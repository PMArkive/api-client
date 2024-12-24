use demostf_client::{ApiClient, Error, ListOrder, ListParams};
use sqlx::postgres::PgPoolOptions;
use std::fs::read;
use std::sync::atomic::{AtomicBool, Ordering};
use steamid_ng::SteamID;
use tracing_subscriber::EnvFilter;

static SETUP_DONE: AtomicBool = AtomicBool::new(false);

fn test_demo_path() -> String {
    std::env::var("TEST_DEMO").unwrap_or_else(|_| "./tests/data/gully.dem".to_string())
}

async fn setup() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    if SETUP_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let db_url = std::env::var("DB_URL")
        .unwrap_or_else(|_| "postgres://postgres:test@localhost:15432/postgres".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .unwrap();

    let tables = [
        "chat",
        "demos",
        "players",
        "storage_keys",
        "teams",
        "upload_blacklist",
        "users",
    ];

    let mut transaction = pool.begin().await.unwrap();

    for table in &tables {
        sqlx::query(&format!("TRUNCATE TABLE {}", table))
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query(&format!("ALTER SEQUENCE {}_id_seq RESTART with 1", table))
            .execute(&mut *transaction)
            .await
            .unwrap();
    }

    sqlx::query("INSERT INTO users(steamid, name, avatar, token)\
    VALUES(76561198024494988, 'Icewind', 'http://cdn.akamai.steamstatic.com/steamcommunity/public/images/avatars/75/75b84075b70535c5cfb3499af03b3e4e7a7b556f_medium.jpg', 'test_token')")
        .execute(&mut *transaction).await.unwrap();

    transaction.commit().await.unwrap();

    let api_root =
        std::env::var("API_ROOT").unwrap_or_else(|_| "http://localhost:8888".to_string());

    let client = ApiClient::with_base_url(&api_root).unwrap();

    upload(&client, &test_demo_path(), "test.dem", "R", "B").await;

    let mut transaction = pool.begin().await.unwrap();

    let views = ["map_list", "name_list", "users_named"];
    for view in views {
        sqlx::query(&format!("REFRESH MATERIALIZED VIEW {}", view))
            .execute(&mut *transaction)
            .await
            .unwrap();
    }

    transaction.commit().await.unwrap();
}

async fn test_client() -> ApiClient {
    setup().await;
    let api_root =
        std::env::var("API_ROOT").unwrap_or_else(|_| "http://localhost:8888".to_string());

    ApiClient::with_base_url(&api_root).unwrap()
}

#[tokio::test]
async fn test_get_user() {
    let client = test_client().await;

    let user = client.get_user(1).await.unwrap();
    assert_eq!("Icewind", user.name);
}

async fn upload(client: &ApiClient, source: &str, name: &str, red: &str, blue: &str) -> u32 {
    let data = std::fs::read(source).unwrap();

    client
        .upload_demo(
            name.to_string(),
            data,
            red.to_string(),
            blue.to_string(),
            "test_token".to_string(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn test_upload_invalid_key() {
    let client = test_client().await;

    let data = std::fs::read(test_demo_path()).unwrap();

    let err = client
        .upload_demo(
            "name.dem".to_string(),
            data,
            "red".to_string(),
            "blue".to_string(),
            "wrong_token".to_string(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, Error::InvalidApiKey));
}

#[tokio::test]
async fn test_list_demos() {
    let client = test_client().await;

    let demos = client
        .list(ListParams::default().with_order(ListOrder::Ascending), 1)
        .await
        .unwrap();
    assert_eq!(demos[0].id, 1);
    assert_eq!(demos[0].uploader.id(), 1);
    assert!(demos[0].uploader.user().is_none());
    assert_eq!(
        demos[0].uploader.resolve(&client).await.unwrap().steam_id,
        SteamID::from(76561198024494988)
    );

    assert_eq!(demos[0].player_count, 12);
    assert_eq!(demos[0].name, "test.dem");
    assert_eq!(demos[0].red_score, 5);
    assert_eq!(demos[0].blue_score, 3);
}

#[tokio::test]
async fn test_get_demo() {
    let client = test_client().await;

    let demo = client.get(1).await.unwrap();
    assert_eq!(demo.id, 1);
    assert_eq!(demo.uploader.id(), 1);
    assert!(demo.uploader.user().is_some());
    assert_eq!(
        demo.uploader.user().unwrap().steam_id,
        SteamID::from(76561198024494988)
    );
    assert_eq!(
        demo.uploader.resolve(&client).await.unwrap().steam_id,
        SteamID::from(76561198024494988)
    );

    let mut players = demo.players.unwrap();
    players.sort_by(|a, b| {
        a.user
            .steam_id
            .account_id()
            .cmp(&b.user.steam_id.account_id())
    });

    assert_eq!(players[0].user.steam_id, SteamID::from(76561198010628997));
    assert_eq!(players[0].user.name, "freak u ___");
}

#[tokio::test]
async fn test_get_chat() {
    let client = test_client().await;

    let chat = client.get_chat(1).await.unwrap();

    assert_eq!(chat.len(), 199);

    assert_eq!(chat[0].user, "distraughtduck4");
    assert_eq!(chat[0].time, 0);
    assert_eq!(chat[0].message, "[P-REC] Recording...");
}

#[tokio::test]
async fn test_get_players() {
    let client = test_client().await;

    let demos = client
        .list(ListParams::default().with_order(ListOrder::Ascending), 1)
        .await
        .unwrap();

    assert!(demos[0].players.is_none());
    assert_eq!(demos[0].get_players(&client).await.unwrap().len(), 12);
}

#[tokio::test]
async fn test_set_url_invalid_key() {
    let client = test_client().await;

    let res = client
        .set_url(
            1,
            "tests",
            "tests",
            "http://example.com/tests",
            [0; 16],
            "wrong",
        )
        .await;
    assert!(matches!(res.unwrap_err(), Error::InvalidApiKey));
}

#[tokio::test]
async fn test_set_url_invalid_hash() {
    let client = test_client().await;

    let res = client
        .set_url(
            1,
            "tests",
            "tests",
            "http://example.com/tests",
            [0; 16],
            "edit",
        )
        .await;
    assert!(matches!(res.unwrap_err(), Error::HashMisMatch));
}

#[tokio::test]
async fn test_set_url_unknown_demo() {
    let client = test_client().await;

    let res = client
        .set_url(
            99,
            "tests",
            "tests",
            "http://example.com/tests",
            [0; 16],
            "edit",
        )
        .await;
    dbg!(&res);
    assert!(matches!(res.unwrap_err(), Error::DemoNotFound(99)));
}

#[tokio::test]
async fn test_set_url() {
    let client = test_client().await;
    let demo = client.get(1).await.unwrap();

    client
        .set_url(
            1,
            "example",
            "tests",
            "http://example.com/tests",
            demo.hash,
            "edit",
        )
        .await
        .unwrap();

    let moved = client.get(1).await.unwrap();

    assert_eq!(moved.backend, "example");
    assert_eq!(moved.path, "tests");
    assert_eq!(moved.url, "http://example.com/tests");
}

#[tokio::test]
async fn test_get_demo_not_found() {
    let client = test_client().await;

    assert!(matches!(
        dbg!(client.get(999).await.unwrap_err()),
        Error::DemoNotFound(999)
    ));
}

#[tokio::test]
async fn test_list_upload() {
    let client = test_client().await;

    let demos = client
        .list_uploads(
            SteamID::from(76561198024494987),
            ListParams::default().with_order(ListOrder::Ascending),
            1,
        )
        .await
        .unwrap();
    assert_eq!(demos.len(), 0);

    let demos = client
        .list_uploads(
            SteamID::from(76561198024494988),
            ListParams::default().with_order(ListOrder::Ascending),
            1,
        )
        .await
        .unwrap();
    assert_eq!(demos[0].id, 1);
}

#[tokio::test]
async fn test_list_players() {
    let client = test_client().await;

    let demos = client
        .list(ListParams::default().with_players([76561198010628997]), 1)
        .await
        .unwrap();
    assert_eq!(demos.len(), 1);
    assert_eq!(demos[0].id, 1);

    let demos = client
        .list(
            ListParams::default().with_players([76561198010628997, 76561198111527393]),
            1,
        )
        .await
        .unwrap();
    assert_eq!(demos.len(), 1);
    assert_eq!(demos[0].id, 1);

    let demos = client
        .list(ListParams::default().with_players([76561198010628990]), 1)
        .await
        .unwrap();
    assert_eq!(demos.len(), 0);
}

#[tokio::test]
async fn test_search_players() {
    let client = test_client().await;

    let user = client.search_users("freak").await.unwrap();
    assert_eq!(user.len(), 1);
    assert_eq!(user[0].steam_id, SteamID::from(76561198010628997));
}

#[tokio::test]
async fn test_download_demo() {
    let client = test_client().await;

    let mut demo = client.get(1).await.unwrap();

    let demos_url =
        std::env::var("API_ROOT").unwrap_or_else(|_| "http://localhost:8888/".to_string());

    // fixup the url to one that is actually usable
    demo.url = format!(
        "{}static/01/b2/01b2265d875026b91d59a2785abfd50d_test.dem",
        demos_url
    );

    let mut data: Vec<u8> = Vec::new();
    demo.save(&client, &mut data).await.unwrap();

    assert_eq!(data.len(), read(test_demo_path()).unwrap().len());
}
