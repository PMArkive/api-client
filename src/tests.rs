use crate::{ApiClient, ListParams, ListOrder, Error};
use steamid_ng::SteamID;

#[tokio::test]
async fn test_list_demos() {
    let client = ApiClient::default();

    let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await.unwrap();
    assert_eq!(demos[0].id, 9);
    assert_eq!(demos[0].uploader.id(), 1);
    assert!(demos[0].uploader.user().is_none());
    assert_eq!(demos[0].uploader.resolve(&client).await.unwrap().steam_id, SteamID::from(76561198024494988));
}

#[tokio::test]
async fn test_get_demo() {
    let client = ApiClient::default();

    let demo = client.get(9).await.unwrap();
    assert_eq!(demo.id, 9);
    assert_eq!(demo.uploader.id(), 1);
    assert!(demo.uploader.user().is_some());
    assert_eq!(demo.uploader.user().unwrap().steam_id, SteamID::from(76561198024494988));
    assert_eq!(demo.uploader.resolve(&client).await.unwrap().steam_id, SteamID::from(76561198024494988));

    assert_eq!(demo.players[0].player_id, 623);
    assert_eq!(demo.players[0].user.id, 346);
}

#[tokio::test]
async fn test_get_chat() {
    let client = ApiClient::default();

    let chat = client.get_chat(447678).await.unwrap();

    assert_eq!(chat.len(), 10);

    assert_eq!(chat[0].user, "wiitabix");
    assert_eq!(chat[0].time, 5);
    assert_eq!(chat[0].message, "gl hf :)))))");
}

#[tokio::test]
async fn test_get_players() {
    let client = ApiClient::default();

    let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await.unwrap();

    assert_eq!(demos[0].players.len(), 0);
    assert_eq!(demos[0].get_players(&client).await.unwrap().len(), 12);
}

#[tokio::test]
async fn test_set_url_invalid_key() {
    let client = ApiClient::default();

    let res = client.set_url(9, "test", "test", "http://example.com/test", [0; 16], "wrong").await;
    assert!(matches!(res.unwrap_err(), Error::InvalidApiKey));
}