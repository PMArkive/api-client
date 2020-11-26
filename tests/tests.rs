use demostf_client::{ApiClient, ListParams, ListOrder, Error};
use steamid_ng::SteamID;
use sqlx::postgres::PgPoolOptions;

async fn test_client() -> ApiClient {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://postgres:test@localhost:15432/postgres").await.unwrap();

    let tables = ["chat", "demos", "kills", "players", "storage_keys", "teams", "upload_blacklist", "users"];

    let mut transaction = pool.begin().await.unwrap();

    for table in &tables {
        sqlx::query(&format!("TRUNCATE TABLE {}", table)).execute(&mut transaction).await.unwrap();
        sqlx::query(&format!("ALTER SEQUENCE {}_id_seq RESTART with 1", table)).execute(&mut transaction).await.unwrap();
    }

    sqlx::query("INSERT INTO users(steamid, name, avatar, token)\
    VALUES(76561198024494988, 'Icewind', 'http://cdn.akamai.steamstatic.com/steamcommunity/public/images/avatars/75/75b84075b70535c5cfb3499af03b3e4e7a7b556f_medium.jpg', 'test_token')")
        .execute(&mut transaction).await.unwrap();

    transaction.commit().await.unwrap();

    ApiClient::with_base_url("http://localhost:8888").unwrap()
}

#[tokio::test]
async fn test_get_user() {
    let client = test_client().await;

    let user = client.get_user(1).await.unwrap();
    assert_eq!("Icewind", user.name);
}

async fn upload(client: &ApiClient, source: &str, name: &str, red: &str, blue: &str) -> u32 {
    let data = std::fs::read(source).unwrap();

    client.upload_demo(name.to_string(), data, red.to_string(), blue.to_string(), "test_token".to_string()).await.unwrap()
}

#[tokio::test]
async fn test_upload_invalid_key() {
    let client = test_client().await;

    let data = std::fs::read("./tests/data/gully.dem").unwrap();

    let err = client.upload_demo("name.dem".to_string(), data, "red".to_string(), "blue".to_string(), "wrong_token".to_string()).await.unwrap_err();

    assert!(matches!(err, Error::InvalidApiKey));
}

#[tokio::test]
async fn test_list_demos() {
    let client = test_client().await;


    let id = upload(&client, "./tests/data/gully.dem", "test.dem", "R", "B").await;

    assert_eq!(1, id);

    let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await.unwrap();
    assert_eq!(demos[0].id, 1);
    assert_eq!(demos[0].uploader.id(), 1);
    assert!(demos[0].uploader.user().is_none());
    assert_eq!(demos[0].uploader.resolve(&client).await.unwrap().steam_id, SteamID::from(76561198024494988));

    assert_eq!(demos[0].player_count, 12);
    assert_eq!(demos[0].name, "test.dem");
    assert_eq!(demos[0].red_score, 5);
    assert_eq!(demos[0].blue_score, 3);
}

#[tokio::test]
async fn test_get_demo() {
    let client = test_client().await;

    let id = upload(&client, "./tests/data/gully.dem", "test.dem", "R", "B").await;

    let demo = client.get(id).await.unwrap();
    assert_eq!(demo.id, id);
    assert_eq!(demo.uploader.id(), 1);
    assert!(demo.uploader.user().is_some());
    assert_eq!(demo.uploader.user().unwrap().steam_id, SteamID::from(76561198024494988));
    assert_eq!(demo.uploader.resolve(&client).await.unwrap().steam_id, SteamID::from(76561198024494988));

    assert_eq!(demo.players[0].player_id, 1);
    assert_eq!(demo.players[0].user.id, 2);
    assert_eq!(demo.players[0].user.name, "distraughtduck4");
}

#[tokio::test]
async fn test_get_chat() {
    let client = test_client().await;

    upload(&client, "./tests/data/gully.dem", "test.dem", "R", "B").await;

    let chat = client.get_chat(1).await.unwrap();

    assert_eq!(chat.len(), 134);

    assert_eq!(chat[0].user, "distraughtduck4");
    assert_eq!(chat[0].time, 0);
    assert_eq!(chat[0].message, "[P-REC] Recording...");
}

#[tokio::test]
async fn test_get_players() {
    let client = test_client().await;

    upload(&client, "./tests/data/gully.dem", "test.dem", "R", "B").await;

    let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await.unwrap();

    assert_eq!(demos[0].players.len(), 0);
    assert_eq!(demos[0].get_players(&client).await.unwrap().len(), 12);
}

#[tokio::test]
async fn test_set_url_invalid_key() {
    let client = test_client().await;

    let res = client.set_url(9, "tests", "tests", "http://example.com/tests", [0; 16], "wrong").await;
    assert!(matches!(res.unwrap_err(), Error::InvalidApiKey));
}