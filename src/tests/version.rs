use std::collections::HashMap;
use rustc_serialize::json::Json;

use conduit::{Handler, Method};
use semver;

use cargo_registry::db::RequestTransaction;
use cargo_registry::version::{EncodableVersion, Version, EncodableBuildInfo};

#[derive(RustcDecodable)]
struct VersionList { versions: Vec<EncodableVersion> }
#[derive(RustcDecodable)]
struct VersionResponse { version: EncodableVersion }

fn sv(s: &str) -> semver::Version {
    semver::Version::parse(s).unwrap()
}

#[test]
fn index() {
    let (_b, app, middle) = ::app();
    let mut req = ::req(app, Method::Get, "/api/v1/versions");
    let mut response = ok_resp!(middle.call(&mut req));
    let json: VersionList = ::json(&mut response);
    assert_eq!(json.versions.len(), 0);

    let (v1, v2) = {
        ::mock_user(&mut req, ::user("foo"));
        let (c, _) = ::mock_crate(&mut req, ::krate("foo_vers_index"));
        let tx = req.tx().unwrap();
        let m = HashMap::new();
        let v1 = Version::insert(tx, c.id, &sv("2.0.0"), &m, &[]).unwrap();
        let v2 = Version::insert(tx, c.id, &sv("2.0.1"), &m, &[]).unwrap();
        (v1, v2)
    };
    req.with_query(&format!("ids[]={}&ids[]={}", v1.id, v2.id));
    let mut response = ok_resp!(middle.call(&mut req));
    let json: VersionList = ::json(&mut response);
    assert_eq!(json.versions.len(), 2);
}

#[test]
fn show() {
    let (_b, app, middle) = ::app();
    let mut req = ::req(app.clone(), Method::Get, "/api/v1/versions");
    let v = {
        let conn = app.diesel_database.get().unwrap();
        let user = ::new_user("foo").create_or_update(&conn).unwrap();
        let krate = ::CrateBuilder::new("foo_vers_show", user.id)
            .expect_build(&conn);
        ::new_version(krate.id, "2.0.0").save(&conn, &[]).unwrap()
    };
    req.with_path(&format!("/api/v1/versions/{}", v.id));
    let mut response = ok_resp!(middle.call(&mut req));
    let json: VersionResponse = ::json(&mut response);
    assert_eq!(json.version.id, v.id);
}

#[test]
fn authors() {
    let (_b, app, middle) = ::app();
    let mut req = ::req(app, Method::Get, "/api/v1/crates/foo_authors/1.0.0/authors");
    ::mock_user(&mut req, ::user("foo"));
    ::mock_crate(&mut req, ::krate("foo_authors"));
    let mut response = ok_resp!(middle.call(&mut req));
    let mut data = Vec::new();
    response.body.write_body(&mut data).unwrap();
    let s = ::std::str::from_utf8(&data).unwrap();
    let json = Json::from_str(&s).unwrap();
    let json = json.as_object().unwrap();
    assert!(json.contains_key(&"users".to_string()));
}

#[test]
fn publish_build_info() {
    #[derive(RustcDecodable)] struct O { ok: bool }
    let (_b, app, middle) = ::app();

    let mut req = ::new_req(app.clone(), "publish-build-info", "1.0.0");

    {
        let conn = app.diesel_database.get().unwrap();
        let user = ::new_user("foo").create_or_update(&conn).unwrap();
        ::CrateBuilder::new("publish-build-info", user.id)
            .version("1.0.0")
            .expect_build(&conn);
        ::sign_in_as(&mut req, &user);
    }

    let body = r#"{
        "name":"publish-build-info",
        "vers":"1.0.0",
        "rust_version":"rustc 1.16.0-nightly (df8debf6d 2017-01-25)",
        "target":"x86_64-pc-windows-gnu",
        "passed":false}"#;

    let mut response = ok_resp!(middle.call(req.with_path(
        "/api/v1/crates/publish-build-info/1.0.0/build_info")
        .with_method(Method::Put)
        .with_body(body.as_bytes())));
    assert!(::json::<O>(&mut response).ok);

    let body = r#"{
        "name":"publish-build-info",
        "vers":"1.0.0",
        "rust_version":"rustc 1.16.0-nightly (df8debf6d 2017-01-25)",
        "target":"x86_64-pc-windows-gnu",
        "passed":true}"#;

    let mut response = ok_resp!(middle.call(req.with_path(
        "/api/v1/crates/publish-build-info/1.0.0/build_info")
        .with_method(Method::Put)
        .with_body(body.as_bytes())));
    assert!(::json::<O>(&mut response).ok);

    let body = r#"{
        "name":"publish-build-info",
        "vers":"1.0.0",
        "rust_version":"rustc 1.13.0 (df8debf6d 2017-01-25)",
        "target":"x86_64-pc-windows-gnu",
        "passed":true}"#;

    let mut response = ok_resp!(middle.call(req.with_path(
        "/api/v1/crates/publish-build-info/1.0.0/build_info")
        .with_method(Method::Put)
        .with_body(body.as_bytes())));
    assert!(::json::<O>(&mut response).ok);

    let body = r#"{
        "name":"publish-build-info",
        "vers":"1.0.0",
        "rust_version":"rustc 1.15.0-beta (df8debf6d 2017-01-20)",
        "target":"x86_64-pc-windows-gnu",
        "passed":true}"#;

    let mut response = ok_resp!(middle.call(req.with_path(
        "/api/v1/crates/publish-build-info/1.0.0/build_info")
        .with_method(Method::Put)
        .with_body(body.as_bytes())));
    assert!(::json::<O>(&mut response).ok);

    let mut response = ok_resp!(middle.call(req.with_path(
        "/api/v1/crates/publish-build-info/1.0.0/build_info")
        .with_method(Method::Get)));

    #[derive(Debug, RustcDecodable)]
    struct R { build_info: EncodableBuildInfo }

    let json = ::json::<R>(&mut response);
    assert_eq!(
        json.build_info.ordering.get("nightly"),
        Some(&vec![String::from("2017-01-25T00:00:00Z")])
    );
    assert_eq!(
        json.build_info.ordering.get("beta"),
        Some(&vec![String::from("2017-01-20T00:00:00Z")])
    );
    assert_eq!(
        json.build_info.ordering.get("stable"),
        Some(&vec![String::from("1.13.0")])
    );
}

#[test]
fn bad_rust_version_publish_build_info() {
    let (_b, app, middle) = ::app();

    let mut req = ::new_req(app.clone(), "bad-rust-vers", "1.0.0");

    {
        let conn = app.diesel_database.get().unwrap();
        let user = ::new_user("foo").create_or_update(&conn).unwrap();
        ::CrateBuilder::new("bad-rust-vers", user.id)
            .version("1.0.0")
            .expect_build(&conn);
        ::sign_in_as(&mut req, &user);
    }

    let body = r#"{
        "name":"bad-rust-vers",
        "vers":"1.0.0",
        "rust_version":"rustc 1.16.0-dev (df8debf6d 2017-01-25)",
        "target":"x86_64-pc-windows-gnu",
        "passed":true}"#;

    let response = bad_resp!(middle.call(req.with_path(
        "/api/v1/crates/bad-rust-vers/1.0.0/build_info")
        .with_method(Method::Put)
        .with_body(body.as_bytes())));

    assert_eq!(
        response.errors[0].detail,
        "rust_version `rustc 1.16.0-dev (df8debf6d 2017-01-25)` \
         not recognized as nightly, beta, or stable");

    let body = r#"{
        "name":"bad-rust-vers",
        "vers":"1.0.0",
        "rust_version":"1.15.0",
        "target":"x86_64-pc-windows-gnu",
        "passed":true}"#;

    let response = bad_resp!(middle.call(req.with_path(
        "/api/v1/crates/bad-rust-vers/1.0.0/build_info")
        .with_method(Method::Put)
        .with_body(body.as_bytes())));

    assert_eq!(
        response.errors[0].detail,
        "rust_version `1.15.0` not recognized; \
        expected format like `rustc X.Y.Z (SHA YYYY-MM-DD)`");
}
