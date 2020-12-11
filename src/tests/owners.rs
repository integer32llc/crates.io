use crate::{
    add_team_to_crate,
    builders::{CrateBuilder, PublishBuilder},
    new_team,
    util::{MockCookieUser, MockTokenUser, RequestHelper},
    TestApp,
};
use cargo_registry::{
    models::Crate,
    views::{EncodableCrateOwnerInvitation, EncodableOwner, InvitationResponse},
};

use conduit::StatusCode;
use diesel::prelude::*;

#[derive(Deserialize)]
struct TeamResponse {
    teams: Vec<EncodableOwner>,
}
#[derive(Deserialize)]
struct UserResponse {
    users: Vec<EncodableOwner>,
}
#[derive(Deserialize)]
struct InvitationListResponse {
    crate_owner_invitations: Vec<EncodableCrateOwnerInvitation>,
}

// Implementing locally for now, unless these are needed elsewhere
impl MockCookieUser {
    /// As the currently logged in user, accept an invitation to become an owner of the named
    /// crate.
    fn accept_ownership_invitation(&self, krate_name: &str, krate_id: i32) {
        let body = json!({
            "crate_owner_invite": {
                "invited_by_username": "",
                "crate_name": krate_name,
                "crate_id": krate_id,
                "created_at": "",
                "accepted": true
            }
        });

        #[derive(Deserialize)]
        struct CrateOwnerInvitation {
            crate_owner_invitation: InvitationResponse,
        }

        let url = format!("/api/v1/me/crate_owner_invitations/{}", krate_id);
        let crate_owner_invite: CrateOwnerInvitation =
            self.put(&url, body.to_string().as_bytes()).good();
        assert!(crate_owner_invite.crate_owner_invitation.accepted);
        assert_eq!(crate_owner_invite.crate_owner_invitation.crate_id, krate_id);
    }

    /// As the currently logged in user, decline an invitation to become an owner of the named
    /// crate.
    fn decline_ownership_invitation(&self, krate_name: &str, krate_id: i32) {
        let body = json!({
            "crate_owner_invite": {
                "invited_by_username": "",
                "crate_name": krate_name,
                "crate_id": krate_id,
                "created_at": "",
                "accepted": false
            }
        });

        #[derive(Deserialize)]
        struct CrateOwnerInvitation {
            crate_owner_invitation: InvitationResponse,
        }

        let url = format!("/api/v1/me/crate_owner_invitations/{}", krate_id);
        let crate_owner_invite: CrateOwnerInvitation =
            self.put(&url, body.to_string().as_bytes()).good();
        assert!(!crate_owner_invite.crate_owner_invitation.accepted);
        assert_eq!(crate_owner_invite.crate_owner_invitation.crate_id, krate_id);
    }

    /// As the currently logged in user, list my pending invitations.
    fn list_invitations(&self) -> InvitationListResponse {
        self.get("/api/v1/me/crate_owner_invitations").good()
    }
}

#[test]
fn new_crate_owner() {
    let (app, _, _, token) = TestApp::full().with_token();

    // Create a crate under one user
    let crate_to_publish = PublishBuilder::new("foo_owner").version("1.0.0");
    token.enqueue_publish(crate_to_publish).good();

    // Add the second user as an owner
    let user2 = app.db_new_user("bar");
    token.add_user_owner("foo_owner", user2.as_model());

    // accept invitation for user to be added as owner
    let krate: Crate = app.db(|conn| Crate::by_name("foo_owner").first(conn).unwrap());
    user2.accept_ownership_invitation("foo_owner", krate.id);

    // Make sure this shows up as one of their crates.
    let crates = user2.search_by_user_id(user2.as_model().id);
    assert_eq!(crates.crates.len(), 1);

    // And upload a new crate as the second user
    let crate_to_publish = PublishBuilder::new("foo_owner").version("2.0.0");
    user2
        .db_new_token("bar_token")
        .enqueue_publish(crate_to_publish)
        .good();
}

#[test]
fn subcrate_permissions() {
    let (app, _, user1, token) = TestApp::full().with_token();

    let namespace_crate_to_publish = PublishBuilder::new("foo").version("1.0.0");
    user1.enqueue_publish(namespace_crate_to_publish).good();

    let subcrate_name = "foo/bar";
    let subcrate_to_publish = PublishBuilder::new(subcrate_name).version("1.0.0");
    user1.enqueue_publish(subcrate_to_publish).good();

    let subcrate: Crate = app.db(|conn| Crate::by_name(subcrate_name).first(conn).unwrap());
    let user2 = create_and_add_owner(&app, &token, "user2", &subcrate);
    let crates = user2.search_by_user_id(user2.as_model().id);
    assert_eq!(crates.crates.len(), 1);
    assert_eq!(crates.crates[0].name, subcrate_name);

    let subcrate_to_publish = PublishBuilder::new(subcrate_name).version("1.0.1");
    user2.enqueue_publish(subcrate_to_publish).good();

    // Owner of a crate should be able to publish its subcrate
    let parent_crate: Crate = app.db(|conn| Crate::by_name("foo").first(conn).unwrap());
    let user3 = create_and_add_owner(&app, &token, "user3", &parent_crate);
    let crate_to_publish = PublishBuilder::new(subcrate_name).version("1.0.2");
    user3.enqueue_publish(crate_to_publish).good();

    // User 2 is an explicit owner of subcrate, but user 3 is not
    assert_eq!(user2.search_by_user_id(user2.as_model().id).crates.len(), 1);
    token.remove_named_owner(subcrate_name, "user2").good();
    assert_eq!(user2.search_by_user_id(user2.as_model().id).crates.len(), 0);

    assert_eq!(user2.search_by_user_id(user3.as_model().id).crates.len(), 2);
    token.remove_named_owner(subcrate_name, "user3").good();
    assert_eq!(user2.search_by_user_id(user3.as_model().id).crates.len(), 2);
}

#[test]
fn subcrate_permissions_rejects_if_user_doesnt_own_namespace() {
    let (app, _, user1) = TestApp::full().with_user();
    let crate_to_publish = PublishBuilder::new("foo").version("1.0.0");
    user1.enqueue_publish(crate_to_publish).good();

    let crate_to_publish = PublishBuilder::new("foo/bar").version("1.0.0");
    let user2 = app.db_new_user("user2");
    let json = user2
        .enqueue_publish(crate_to_publish)
        .bad_with_status(StatusCode::OK);
    assert!(&json.errors[0]
        .detail
        .contains("this crate doesn't exist, but it belongs to a namespace which exists"));
}

#[test]
fn owner_of_namespace_also_owns_subcrates() {
    let (app, _, user1, token) = TestApp::full().with_token();
    let namespace_name = "foo";
    let subcrate_name = "foo/bar";

    let namespace_to_publish = PublishBuilder::new(namespace_name).version("1.0.0");
    user1.enqueue_publish(namespace_to_publish).good();
    let namespace_crate = app.db(|conn| Crate::by_name(namespace_name).first(conn).unwrap());

    let user2 = create_and_add_owner(&app, &token, "user2", &namespace_crate);
    let subcrate_to_publish = PublishBuilder::new(subcrate_name).version("1.0.0");
    user1.enqueue_publish(subcrate_to_publish).good();

    let crates = user2.search_by_user_id(user2.as_model().id);
    assert_eq!(crates.crates.len(), 2);
    assert_eq!(crates.crates[0].name, namespace_name);
    assert_eq!(crates.crates[1].name, subcrate_name);
}

fn create_and_add_owner(
    app: &TestApp,
    token: &MockTokenUser,
    username: &str,
    krate: &Crate,
) -> MockCookieUser {
    let user = app.db_new_user(username);
    token.add_user_owner(&krate.name, user.as_model());
    user.accept_ownership_invitation(&krate.name, krate.id);
    user
}

// Ensures that so long as at least one owner remains associated with the crate,
// a user can still remove their own login as an owner
#[test]
fn owners_can_remove_self() {
    let (app, _, user, token) = TestApp::init().with_token();
    let username = &user.as_model().gh_login;

    let krate = app
        .db(|conn| CrateBuilder::new("owners_selfremove", user.as_model().id).expect_build(conn));

    // Deleting yourself when you're the only owner isn't allowed.
    let json = token
        .remove_named_owner("owners_selfremove", username)
        .bad_with_status(StatusCode::OK);
    assert!(json.errors[0]
        .detail
        .contains("cannot remove all individual owners of a crate"));

    create_and_add_owner(&app, &token, "secondowner", &krate);

    // Deleting yourself when there are other owners is allowed.
    let json = token
        .remove_named_owner("owners_selfremove", username)
        .good();
    assert!(json.ok);

    // After you delete yourself, you no longer have permisions to manage the crate.
    let json = token
        .remove_named_owner("owners_selfremove", username)
        .bad_with_status(StatusCode::OK);
    assert!(json.errors[0]
        .detail
        .contains("only owners have permission to modify owners",));
}

// Verify consistency when adidng or removing multiple owners in a single request.
#[test]
fn modify_multiple_owners() {
    let (app, _, user, token) = TestApp::init().with_token();
    let username = &user.as_model().gh_login;

    let krate =
        app.db(|conn| CrateBuilder::new("owners_multiple", user.as_model().id).expect_build(conn));

    let user2 = create_and_add_owner(&app, &token, "user2", &krate);
    let user3 = create_and_add_owner(&app, &token, "user3", &krate);

    // Deleting all owners is not allowed.
    let json = token
        .remove_named_owners("owners_multiple", &[username, "user2", "user3"])
        .bad_with_status(StatusCode::OK);
    assert!(&json.errors[0]
        .detail
        .contains("cannot remove all individual owners of a crate"));
    assert_eq!(app.db(|conn| krate.owners(&conn).unwrap()).len(), 3);

    // Deleting two owners at once is allowed.
    let json = token
        .remove_named_owners("owners_multiple", &["user2", "user3"])
        .good();
    assert!(json.ok);
    assert_eq!(app.db(|conn| krate.owners(&conn).unwrap()).len(), 1);

    // Adding multiple users fails if one of them already is an owner.
    let json = token
        .add_named_owners("owners_multiple", &["user2", username])
        .bad_with_status(StatusCode::OK);
    assert!(&json.errors[0].detail.contains("is already an owner"));
    assert_eq!(app.db(|conn| krate.owners(&conn).unwrap()).len(), 1);

    // Adding multiple users at once succeeds.
    let json = token
        .add_named_owners("owners_multiple", &["user2", "user3"])
        .good();
    assert!(json.ok);
    user2.accept_ownership_invitation(&krate.name, krate.id);
    user3.accept_ownership_invitation(&krate.name, krate.id);
    assert_eq!(app.db(|conn| krate.owners(&conn).unwrap()).len(), 3);
}

/*  Testing the crate ownership between two crates and one team.
    Given two crates, one crate owned by both a team and a user,
    one only owned by a user, check that the CrateList returned
    for the user_id contains only the crates owned by that user,
    and that the CrateList returned for the team_id contains
    only crates owned by that team.
*/
#[test]
fn check_ownership_two_crates() {
    let (app, anon, user) = TestApp::init().with_user();
    let user = user.as_model();

    let (krate_owned_by_team, team) = app.db(|conn| {
        let t = new_team("team_foo").create_or_update(conn).unwrap();
        let krate = CrateBuilder::new("foo", user.id).expect_build(conn);
        add_team_to_crate(&t, &krate, user, conn).unwrap();
        (krate, t)
    });

    let user2 = app.db_new_user("user_bar");
    let user2 = user2.as_model();
    let krate_not_owned_by_team =
        app.db(|conn| CrateBuilder::new("bar", user2.id).expect_build(conn));

    let json = anon.search_by_user_id(user2.id);
    assert_eq!(json.crates[0].name, krate_not_owned_by_team.name);
    assert_eq!(json.crates.len(), 1);

    let query = format!("team_id={}", team.id);
    let json = anon.search(&query);
    assert_eq!(json.crates.len(), 1);
    assert_eq!(json.crates[0].name, krate_owned_by_team.name);
}

/*  Given a crate owned by both a team and a user, check that the
    JSON returned by the /owner_team route and /owner_user route
    contains the correct kind of owner

    Note that in this case function new_team must take a team name
    of form github:org_name:team_name as that is the format
    EncodableOwner::encodable is expecting
*/
#[test]
fn check_ownership_one_crate() {
    let (app, anon, user) = TestApp::init().with_user();
    let user = user.as_model();

    let team = app.db(|conn| {
        let t = new_team("github:test_org:team_sloth")
            .create_or_update(conn)
            .unwrap();
        let krate = CrateBuilder::new("best_crate", user.id).expect_build(conn);
        add_team_to_crate(&t, &krate, user, conn).unwrap();
        t
    });

    let json: TeamResponse = anon.get("/api/v1/crates/best_crate/owner_team").good();
    assert_eq!(json.teams[0].kind, "team");
    assert_eq!(json.teams[0].name, team.name);

    let json: UserResponse = anon.get("/api/v1/crates/best_crate/owner_user").good();
    assert_eq!(json.users[0].kind, "user");
    assert_eq!(json.users[0].name, user.name);
}

#[test]
fn deleted_ownership_isnt_in_owner_user() {
    let (app, anon, user) = TestApp::init().with_user();
    let user = user.as_model();

    app.db(|conn| {
        let krate = CrateBuilder::new("foo_my_packages", user.id).expect_build(conn);
        krate
            .owner_remove(app.as_inner(), conn, user, &user.gh_login)
            .unwrap();
    });

    let json: UserResponse = anon.get("/api/v1/crates/foo_my_packages/owner_user").good();
    assert_eq!(json.users.len(), 0);
}

#[test]
fn invitations_are_empty_by_default() {
    let (_, _, user) = TestApp::init().with_user();

    let json = user.list_invitations();
    assert_eq!(json.crate_owner_invitations.len(), 0);
}

#[test]
fn invitations_list() {
    let (app, _, owner, token) = TestApp::init().with_token();
    let owner = owner.as_model();

    let krate = app.db(|conn| CrateBuilder::new("invited_crate", owner.id).expect_build(conn));

    let user = app.db_new_user("invited_user");
    token.add_user_owner("invited_crate", user.as_model());

    let json = user.list_invitations();
    assert_eq!(json.crate_owner_invitations.len(), 1);
    assert_eq!(
        json.crate_owner_invitations[0].invited_by_username,
        owner.gh_login
    );
    assert_eq!(json.crate_owner_invitations[0].crate_name, "invited_crate");
    assert_eq!(json.crate_owner_invitations[0].crate_id, krate.id);
}

/*  Given a user inviting a different user to be a crate
    owner, check that the user invited can accept their
    invitation, the invitation will be deleted from
    the invitations table, and a new crate owner will be
    inserted into the table for the given crate.
*/
#[test]
fn test_accept_invitation() {
    let (app, anon, owner, owner_token) = TestApp::init().with_token();
    let owner = owner.as_model();
    let invited_user = app.db_new_user("user_bar");
    let krate = app.db(|conn| CrateBuilder::new("accept_invitation", owner.id).expect_build(conn));

    // Invite a new owner
    owner_token.add_user_owner("accept_invitation", invited_user.as_model());

    // New owner accepts the invitation
    invited_user.accept_ownership_invitation(&krate.name, krate.id);

    // New owner's invitation list should now be empty
    let json = invited_user.list_invitations();
    assert_eq!(json.crate_owner_invitations.len(), 0);

    // New owner is now listed as an owner, so the crate has two owners
    let json = anon.show_crate_owners("accept_invitation");
    assert_eq!(json.users.len(), 2);
}

/*  Given a user inviting a different user to be a crate
    owner, check that the user invited can decline their
    invitation and the invitation will be deleted from
    the invitations table.
*/
#[test]
fn test_decline_invitation() {
    let (app, anon, owner, owner_token) = TestApp::init().with_token();
    let owner = owner.as_model();
    let invited_user = app.db_new_user("user_bar");
    let krate = app.db(|conn| CrateBuilder::new("decline_invitation", owner.id).expect_build(conn));

    // Invite a new owner
    owner_token.add_user_owner("decline_invitation", invited_user.as_model());

    // Invited user declines the invitation
    invited_user.decline_ownership_invitation(&krate.name, krate.id);

    // Invited user's invitation list should now be empty
    let json = invited_user.list_invitations();
    assert_eq!(json.crate_owner_invitations.len(), 0);

    // Invited user is NOT listed as an owner, so the crate still only has one owner
    let json = anon.show_crate_owners("decline_invitation");
    assert_eq!(json.users.len(), 1);
}

#[test]
fn inactive_users_dont_get_invitations() {
    use cargo_registry::models::NewUser;
    use std::borrow::Cow;

    let (app, _, owner, owner_token) = TestApp::init().with_token();
    let owner = owner.as_model();

    // An inactive user with gh_id -1 and an active user with a non-negative gh_id both exist
    let invited_gh_login = "user_bar";
    let krate_name = "inactive_test";

    app.db(|conn| {
        NewUser {
            gh_id: -1,
            gh_login: invited_gh_login,
            name: None,
            gh_avatar: None,
            gh_access_token: Cow::Borrowed("some random token"),
        }
        .create_or_update(None, conn)
        .unwrap();
        CrateBuilder::new(krate_name, owner.id).expect_build(conn);
    });

    let invited_user = app.db_new_user(invited_gh_login);

    owner_token.add_user_owner(krate_name, invited_user.as_model());

    let json = invited_user.list_invitations();
    assert_eq!(json.crate_owner_invitations.len(), 1);
}

#[test]
fn highest_gh_id_is_most_recent_account_we_know_of() {
    let (app, _, owner, owner_token) = TestApp::init().with_token();
    let owner = owner.as_model();

    // An inactive user with a lower gh_id and an active user with a higher gh_id both exist
    let invited_gh_login = "user_bar";
    let krate_name = "newer_user_test";

    // This user will get a lower gh_id, given how crate::new_user works
    app.db_new_user(invited_gh_login);

    let invited_user = app.db_new_user(invited_gh_login);

    app.db(|conn| {
        CrateBuilder::new(krate_name, owner.id).expect_build(conn);
    });

    owner_token.add_user_owner(krate_name, invited_user.as_model());

    let json = invited_user.list_invitations();
    assert_eq!(json.crate_owner_invitations.len(), 1);
}
