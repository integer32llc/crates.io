use std::collections::{HashMap, BTreeSet};
use std::str::FromStr;

use conduit::{Request, Response};
use conduit_router::RequestParams;
use diesel;
use diesel::pg::{Pg, PgConnection};
use diesel::pg::upsert::*;
use diesel::prelude::*;
use pg::GenericConnection;
use pg::rows::Row;
use rustc_serialize::json;
use semver;
use time::{self, Duration, Timespec, now_utc, strptime};
use url;

use app::RequestApp;
use db::RequestTransaction;
use dependency::{Dependency, EncodableDependency};
use download::{VersionDownload, EncodableVersionDownload};
use git;
use owner::{rights, Rights};
use schema::*;
use upload;
use user::RequestUser;
use util::{RequestUtils, CargoError, CargoResult, ChainError, internal, human};
use {Model, Crate};

#[derive(Clone, Identifiable, Associations)]
#[belongs_to(Crate)]
#[has_many(build_info)]
pub struct Version {
    pub id: i32,
    pub crate_id: i32,
    pub num: semver::Version,
    pub updated_at: Timespec,
    pub created_at: Timespec,
    pub downloads: i32,
    pub features: HashMap<String, Vec<String>>,
    pub yanked: bool,
}

#[derive(Insertable)]
#[table_name="versions"]
pub struct NewVersion {
    crate_id: i32,
    num: String,
    features: String,
}

pub struct Author {
    pub name: String
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct EncodableVersion {
    pub id: i32,
    pub krate: String,
    pub num: String,
    pub dl_path: String,
    pub updated_at: String,
    pub created_at: String,
    pub downloads: i32,
    pub features: HashMap<String, Vec<String>>,
    pub yanked: bool,
    pub links: VersionLinks,
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct VersionLinks {
    pub dependencies: String,
    pub version_downloads: String,
    pub authors: String,
}

#[derive(Insertable, AsChangeset)]
#[table_name="build_info"]
#[primary_key(version_id, rust_version, target)]
struct NewBuildInfo {
    version_id: i32,
    rust_version: String,
    target: String,
    passed: bool,
}

#[derive(Identifiable, Queryable, Associations)]
#[belongs_to(Version)]
#[table_name="build_info"]
#[primary_key(version_id, rust_version, target)]
struct BuildInfo {
    version_id: i32,
    rust_version: String,
    target: String,
    passed: bool,
}

#[derive(RustcEncodable, RustcDecodable, Default)]
pub struct EncodableBuildInfo {
    id: i32,
    pub ordering: HashMap<String, Vec<String>>,
    pub stable: HashMap<String, HashMap<String, bool>>,
    pub beta: HashMap<String, HashMap<String, bool>>,
    pub nightly: HashMap<String, HashMap<String, bool>>,
}

pub enum ChannelVersion {
    Stable(semver::Version),
    Beta(Timespec),
    Nightly(Timespec),
}

impl FromStr for ChannelVersion {
    type Err = Box<CargoError>;

    fn from_str(s: &str) -> CargoResult<Self> {
        // Recognized formats:
        // rustc 1.14.0 (e8a012324 2016-12-16)
        // rustc 1.15.0-beta.5 (10893a9a3 2017-01-19)
        // rustc 1.16.0-nightly (df8debf6d 2017-01-25)

        let pieces: Vec<_> = s.split(&[' ', '(', ')'][..])
                              .filter(|s| !s.trim().is_empty())
                              .collect();
        if pieces.len() != 4 {
            return Err(human(&format_args!(
                "rust_version `{}` not recognized; \
                expected format like `rustc X.Y.Z (SHA YYYY-MM-DD)`",
                s
            )));
        }

        if pieces[1].contains("nightly") {
            Ok(ChannelVersion::Nightly(time::strptime(pieces[3], "%Y-%m-%d")?.to_timespec()))
        } else if pieces[1].contains("beta") {
            Ok(ChannelVersion::Beta(time::strptime(pieces[3], "%Y-%m-%d")?.to_timespec()))
        } else {
            let v = semver::Version::parse(pieces[1])?;
            if v.pre.is_empty() {
                Ok(ChannelVersion::Stable(v))
            } else {
                Err(human(&format_args!(
                    "rust_version `{}` not recognized as nightly, beta, or stable",
                    s
                )))
            }
        }
    }
}

impl Version {
    pub fn find_by_num(conn: &GenericConnection,
                       crate_id: i32,
                       num: &semver::Version)
                       -> CargoResult<Option<Version>> {
        let num = num.to_string();
        let stmt = conn.prepare("SELECT * FROM versions \
                                      WHERE crate_id = $1 AND num = $2")?;
        let rows = stmt.query(&[&crate_id, &num])?;
        Ok(rows.iter().next().map(|r| Model::from_row(&r)))
    }

    pub fn insert(conn: &GenericConnection,
                  crate_id: i32,
                  num: &semver::Version,
                  features: &HashMap<String, Vec<String>>,
                  authors: &[String])
                  -> CargoResult<Version> {
        let num = num.to_string();
        let features = json::encode(features).unwrap();
        let stmt = conn.prepare("INSERT INTO versions \
                                      (crate_id, num, features) \
                                      VALUES ($1, $2, $3) \
                                      RETURNING *")?;
        let rows = stmt.query(&[&crate_id, &num, &features])?;
        let ret: Version = Model::from_row(&rows.iter().next().chain_error(|| {
            internal("no version returned")
        })?);
        for author in authors {
            ret.add_author(conn, author)?;
        }
        Ok(ret)
    }

    pub fn valid(version: &str) -> bool {
        semver::Version::parse(version).is_ok()
    }

    pub fn encodable(self, crate_name: &str) -> EncodableVersion {
        let Version { id, num, updated_at, created_at,
                      downloads, features, yanked, .. } = self;
        let num = num.to_string();
        EncodableVersion {
            dl_path: format!("/api/v1/crates/{}/{}/download", crate_name, num),
            num: num.clone(),
            id: id,
            krate: crate_name.to_string(),
            updated_at: ::encode_time(updated_at),
            created_at: ::encode_time(created_at),
            downloads: downloads,
            features: features,
            yanked: yanked,
            links: VersionLinks {
                dependencies: format!("/api/v1/crates/{}/{}/dependencies",
                                      crate_name, num),
                version_downloads: format!("/api/v1/crates/{}/{}/downloads",
                                           crate_name, num),
                authors: format!("/api/v1/crates/{}/{}/authors", crate_name, num),
            },
        }
    }

    /// Returns (dependency, crate dependency name)
    pub fn dependencies(&self, conn: &GenericConnection)
                        -> CargoResult<Vec<(Dependency, String)>> {
        let stmt = conn.prepare("SELECT dependencies.*,
                                             crates.name AS crate_name
                                      FROM dependencies
                                      LEFT JOIN crates
                                        ON crates.id = dependencies.crate_id
                                      WHERE dependencies.version_id = $1
                                      ORDER BY optional, name")?;
        let rows = stmt.query(&[&self.id])?;
        Ok(rows.iter().map(|r| {
            (Model::from_row(&r), r.get("crate_name"))
        }).collect())
    }

    pub fn authors(&self, conn: &GenericConnection) -> CargoResult<Vec<Author>> {
        let stmt = conn.prepare("SELECT * FROM version_authors
                                       WHERE version_id = $1
                                       ORDER BY name ASC")?;
        let rows = stmt.query(&[&self.id])?;
        Ok(rows.into_iter().map(|row| {
            Author { name: row.get("name") }
        }).collect())
    }

    pub fn add_author(&self,
                      conn: &GenericConnection,
                      name: &str) -> CargoResult<()> {
        conn.execute("INSERT INTO version_authors (version_id, name)
                           VALUES ($1, $2)", &[&self.id, &name])?;
        Ok(())
    }

    pub fn yank(&self, conn: &GenericConnection, yanked: bool) -> CargoResult<()> {
        conn.execute("UPDATE versions SET yanked = $1 WHERE id = $2",
                     &[&yanked, &self.id])?;
        Ok(())
    }

    pub fn max<T>(versions: T) -> semver::Version where
        T: IntoIterator<Item=semver::Version>,
    {
        versions.into_iter()
            .max()
            .unwrap_or_else(|| semver::Version {
                major: 0,
                minor: 0,
                patch: 0,
                pre: vec![],
                build: vec![],
            })
    }

    pub fn store_build_info(&self,
                            conn: &PgConnection,
                            info: upload::VersionBuildInfo) -> CargoResult<()> {

        // Verify specified Rust version will parse before doing any inserting
        info.channel_version()?;

        let bi = NewBuildInfo {
            version_id: self.id,
            rust_version: info.rust_version,
            target: info.target,
            passed: info.passed,
        };

        diesel::insert(&bi.on_conflict(
            build_info::table.primary_key(),
            do_update()
                .set((build_info::passed.eq(excluded(build_info::passed)),
                      build_info::updated_at.eq(now_utc().to_timespec())))
        ))
            .into(build_info::table)
            .execute(conn)?;

        Ok(())
    }
}

impl NewVersion {
    pub fn new(
        crate_id: i32,
        num: &semver::Version,
        features: &HashMap<String, Vec<String>>,
    ) -> CargoResult<Self> {
        let features = json::encode(features)?;
        Ok(NewVersion {
            crate_id: crate_id,
            num: num.to_string(),
            features: features,
        })
    }

    pub fn save(&self, conn: &PgConnection, authors: &[String]) -> CargoResult<Version> {
        use diesel::{select, insert};
        use diesel::expression::dsl::exists;
        use schema::versions::dsl::*;

        let already_uploaded = versions.filter(crate_id.eq(self.crate_id))
            .filter(num.eq(&self.num));
        if select(exists(already_uploaded)).get_result(conn)? {
            return Err(human(&format_args!("crate version `{}` is already \
                                           uploaded", self.num)));
        }

        conn.transaction(|| {
            let version = insert(self).into(versions)
                .get_result::<Version>(conn)?;

            let new_authors = authors.iter().map(|s| NewAuthor {
                version_id: version.id,
                name: &*s,
            }).collect::<Vec<_>>();

            insert(&new_authors).into(version_authors::table)
                .execute(conn)?;
            Ok(version)
        })
    }
}

#[derive(Insertable)]
#[table_name="version_authors"]
struct NewAuthor<'a> {
    version_id: i32,
    name: &'a str,
}

impl Queryable<versions::SqlType, Pg> for Version {
    type Row = (i32, i32, String, Timespec, Timespec, i32, Option<String>, bool);

    fn build(row: Self::Row) -> Self {
        let features = row.6.map(|s| {
            json::decode(&s).unwrap()
        }).unwrap_or_else(HashMap::new);
        Version {
            id: row.0,
            crate_id: row.1,
            num: semver::Version::parse(&row.2).unwrap(),
            updated_at: row.3,
            created_at: row.4,
            downloads: row.5,
            features: features,
            yanked: row.7,
        }
    }
}

impl Model for Version {
    fn from_row(row: &Row) -> Version {
        let num: String = row.get("num");
        let features: Option<String> = row.get("features");
        let features = features.map(|s| {
            json::decode(&s).unwrap()
        }).unwrap_or_else(HashMap::new);
        Version {
            id: row.get("id"),
            crate_id: row.get("crate_id"),
            num: semver::Version::parse(&num).unwrap(),
            updated_at: row.get("updated_at"),
            created_at: row.get("created_at"),
            downloads: row.get("downloads"),
            features: features,
            yanked: row.get("yanked"),
        }
    }
    fn table_name(_: Option<Version>) -> &'static str { "versions" }
}

/// Handles the `GET /versions` route.
// FIXME: where/how is this used?
pub fn index(req: &mut Request) -> CargoResult<Response> {
    let conn = req.tx()?;

    // Extract all ids requested.
    let query = url::form_urlencoded::parse(req.query_string().unwrap_or("")
                                               .as_bytes());
    let ids = query.filter_map(|(ref a, ref b)| {
        if *a == "ids[]" {
            b.parse().ok()
        } else {
            None
        }
    }).collect::<Vec<i32>>();

    // Load all versions
    //
    // TODO: can rust-postgres do this for us?
    let mut versions = Vec::new();
    if !ids.is_empty() {
        let stmt = conn.prepare("\
            SELECT versions.*, crates.name AS crate_name
              FROM versions
            LEFT JOIN crates ON crates.id = versions.crate_id
            WHERE versions.id = ANY($1)
        ")?;
        for row in stmt.query(&[&ids])?.iter() {
            let v: Version = Model::from_row(&row);
            let crate_name: String = row.get("crate_name");
            versions.push(v.encodable(&crate_name));
        }
    }

    #[derive(RustcEncodable)]
    struct R { versions: Vec<EncodableVersion> }
    Ok(req.json(&R { versions: versions }))
}

/// Handles the `GET /versions/:version_id` route.
pub fn show(req: &mut Request) -> CargoResult<Response> {
    let (version, krate) = match req.params().find("crate_id") {
        Some(..) => version_and_crate(req)?,
        None => {
            let id = &req.params()["version_id"];
            let id = id.parse().unwrap_or(0);
            let conn = req.db_conn()?;
            versions::table.find(id)
                .inner_join(crates::table)
                .select((versions::all_columns, ::krate::ALL_COLUMNS))
                .first(&*conn)?
        }
    };

    #[derive(RustcEncodable)]
    struct R { version: EncodableVersion }
    Ok(req.json(&R { version: version.encodable(&krate.name) }))
}

fn version_and_crate_old(req: &mut Request) -> CargoResult<(Version, Crate)> {
    let crate_name = &req.params()["crate_id"];
    let semver = &req.params()["version"];
    let semver = semver::Version::parse(semver).map_err(|_| {
        human(&format_args!("invalid semver: {}", semver))
    })?;
    let tx = req.tx()?;
    let krate = Crate::find_by_name(tx, crate_name)?;
    let version = Version::find_by_num(tx, krate.id, &semver)?;
    let version = version.chain_error(|| {
        human(&format_args!("crate `{}` does not have a version `{}`",
                      crate_name, semver))
    })?;
    Ok((version, krate))
}

fn version_and_crate(req: &mut Request) -> CargoResult<(Version, Crate)> {
    let crate_name = &req.params()["crate_id"];
    let semver = &req.params()["version"];
    if semver::Version::parse(semver).is_err() {
        return Err(human(&format_args!("invalid semver: {}", semver)));
    };
    let conn = req.db_conn()?;
    let krate = Crate::by_name(crate_name).first::<Crate>(&*conn)?;
    let version = Version::belonging_to(&krate)
        .filter(versions::num.eq(semver))
        .first(&*conn)
        .map_err(|_| {
            human(&format_args!("crate `{}` does not have a version `{}`",
                          crate_name, semver))
        })?;
    Ok((version, krate))
}

/// Handles the `GET /crates/:crate_id/:version/dependencies` route.
pub fn dependencies(req: &mut Request) -> CargoResult<Response> {
    let (version, _) = version_and_crate_old(req)?;
    let tx = req.tx()?;
    let deps = version.dependencies(tx)?;
    let deps = deps.into_iter().map(|(dep, crate_name)| {
        dep.encodable(&crate_name, None)
    }).collect();

    #[derive(RustcEncodable)]
    struct R { dependencies: Vec<EncodableDependency> }
    Ok(req.json(&R{ dependencies: deps }))
}

/// Handles the `GET /crates/:crate_id/:version/downloads` route.
pub fn downloads(req: &mut Request) -> CargoResult<Response> {
    let (version, _) = version_and_crate_old(req)?;
    let cutoff_end_date = req.query().get("before_date")
        .and_then(|d| strptime(d, "%Y-%m-%d").ok())
        .unwrap_or_else(now_utc).to_timespec();
    let cutoff_start_date = cutoff_end_date + Duration::days(-89);

    let tx = req.tx()?;
    let stmt = tx.prepare("SELECT * FROM version_downloads
                                WHERE date BETWEEN date($1) AND date($2) AND version_id = $3
                                ORDER BY date ASC")?;
    let downloads = stmt.query(&[&cutoff_start_date, &cutoff_end_date, &version.id])?
        .iter().map(|row| VersionDownload::from_row(&row).encodable()).collect();

    #[derive(RustcEncodable)]
    struct R { version_downloads: Vec<EncodableVersionDownload> }
    Ok(req.json(&R{ version_downloads: downloads }))
}

/// Handles the `GET /crates/:crate_id/:version/build_info` route.
pub fn build_info(req: &mut Request) -> CargoResult<Response> {
    let (version, _) = try!(version_and_crate(req));

    let conn = req.db_conn()?;

    let build_infos = BuildInfo::belonging_to(&version)
        .select((build_info::version_id,
                 build_info::rust_version,
                 build_info::target,
                 build_info::passed))
        .load(&*conn)?;

    let mut encodable_build_info = EncodableBuildInfo::default();
    encodable_build_info.id = version.id;
    let mut stables = BTreeSet::new();
    let mut betas = BTreeSet::new();
    let mut nightlies = BTreeSet::new();

    for row in build_infos {
        let BuildInfo { rust_version, target, passed, .. } = row;
        let rust_version: ChannelVersion = rust_version.parse()?;

        match rust_version {
            ChannelVersion::Stable(semver) => {
                let key = semver.to_string();
                stables.insert(semver);
                encodable_build_info.stable.entry(key)
                                 .or_insert_with(HashMap::new)
                                 .insert(target, passed);
            }
            ChannelVersion::Beta(date) => {
                let key = ::encode_time(date);
                betas.insert(date);
                encodable_build_info.beta.entry(key)
                                  .or_insert_with(HashMap::new)
                                  .insert(target, passed);
            }
            ChannelVersion::Nightly(date) => {
                let key = ::encode_time(date);
                nightlies.insert(date);
                encodable_build_info.nightly.entry(key)
                                  .or_insert_with(HashMap::new)
                                  .insert(target, passed);
            }
        }
    }

    encodable_build_info.ordering.insert(
        String::from("stable"),
        stables.into_iter().map(|s| s.to_string()).collect()
    );

    encodable_build_info.ordering.insert(
        String::from("beta"),
        betas.into_iter().map(::encode_time).collect()
    );

    encodable_build_info.ordering.insert(
        String::from("nightly"),
        nightlies.into_iter().map(::encode_time).collect()
    );

    #[derive(RustcEncodable)]
    struct R { build_info: EncodableBuildInfo }
    Ok(req.json(&R{ build_info: encodable_build_info }))
}

/// Handles the `GET /crates/:crate_id/:version/authors` route.
pub fn authors(req: &mut Request) -> CargoResult<Response> {
    let (version, _) = version_and_crate_old(req)?;
    let tx = req.tx()?;
    let names = version.authors(tx)?.into_iter().map(|a| a.name).collect();

    // It was imagined that we wold associate authors with users.
    // This was never implemented. This complicated return struct
    // is all that is left, hear for backwards compatibility.
    #[derive(RustcEncodable)]
    struct R { users: Vec<::user::EncodableUser>, meta: Meta }
    #[derive(RustcEncodable)]
    struct Meta { names: Vec<String> }
    Ok(req.json(&R{ users: vec![], meta: Meta { names: names } }))
}

/// Handles the `DELETE /crates/:crate_id/:version/yank` route.
pub fn yank(req: &mut Request) -> CargoResult<Response> {
    modify_yank(req, true)
}

/// Handles the `PUT /crates/:crate_id/:version/unyank` route.
pub fn unyank(req: &mut Request) -> CargoResult<Response> {
    modify_yank(req, false)
}

fn modify_yank(req: &mut Request, yanked: bool) -> CargoResult<Response> {
    let (version, krate) = version_and_crate(req)?;
    let user = req.user()?;
    let conn = req.db_conn()?;
    let owners = krate.owners(&conn)?;
    if rights(req.app(), &owners, user)? < Rights::Publish {
        return Err(human("must already be an owner to yank or unyank"))
    }

    if version.yanked != yanked {
        conn.transaction::<_, Box<CargoError>, _>(|| {
            diesel::update(&version).set(versions::yanked.eq(yanked))
                .execute(&*conn)?;
            git::yank(&**req.app(), &krate.name, &version.num, yanked)?;
            Ok(())
        })?;
    }

    #[derive(RustcEncodable)]
    struct R { ok: bool }
    Ok(req.json(&R{ ok: true }))
}

/// Handles the `POST /crates/:crate_id/:version/build_info` route.
pub fn publish_build_info(req: &mut Request) -> CargoResult<Response> {
    let mut body = String::new();
    try!(req.body().read_to_string(&mut body));
    let info: upload::VersionBuildInfo = try!(json::decode(&body).map_err(|e| {
        human(&format_args!("invalid upload request: {:?}", e))
    }));

    let (version, krate) = try!(version_and_crate(req));
    let user = try!(req.user());
    let tx = try!(req.db_conn());
    let owners = try!(krate.owners(&tx));
    if try!(rights(req.app(), &owners, user)) < Rights::Publish {
        return Err(human("must already be an owner to publish build info"))
    }

    version.store_build_info(&tx, info)?;

    #[derive(RustcEncodable)]
    struct R { ok: bool }
    Ok(req.json(&R{ ok: true }))
}
