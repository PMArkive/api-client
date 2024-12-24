use bytes::Bytes;
pub use client::ApiClient;
use futures_util::{Stream, StreamExt};
use md5::Context;
use reqwest::StatusCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Write;
pub use steamid_ng::SteamID;
use thiserror::Error;
use time::OffsetDateTime;
use tinyvec::TinyVec;
use tracing::{debug, error, instrument};

mod client;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Invalid base url")]
    InvalidBaseUrl,
    #[error("Request failed: {0}")]
    Request(reqwest::Error),
    #[error("Invalid page requested")]
    InvalidPage,
    #[error("Invalid api key")]
    InvalidApiKey,
    #[error("Hash mismatch")]
    HashMisMatch,
    #[error("Unknown server error {0}")]
    ServerError(u16),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Demo {0} not found")]
    DemoNotFound(u32),
    #[error("User {0} not found")]
    UserNotFound(u32),
    #[error("Error while writing demo data")]
    Write(#[source] std::io::Error),
    #[error("Operation timed out")]
    TimeOut,
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Error::TimeOut
        } else {
            match error.status() {
                Some(StatusCode::UNAUTHORIZED) => Error::InvalidApiKey,
                Some(StatusCode::PRECONDITION_FAILED) => Error::HashMisMatch,
                Some(status) if status.is_server_error() => Error::ServerError(status.as_u16()),
                _ => Error::Request(error),
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Data of an uploaded demo
pub struct Demo {
    pub id: u32,
    pub url: String,
    pub name: String,
    pub server: String,
    pub duration: u16,
    pub nick: String,
    pub map: String,
    #[serde(with = "time::serde::timestamp")]
    pub time: OffsetDateTime,
    pub red: String,
    pub blue: String,
    pub red_score: u8,
    pub blue_score: u8,
    pub player_count: u8,
    pub uploader: UserRef,
    #[serde(deserialize_with = "hex_to_digest")]
    pub hash: [u8; 16],
    pub backend: String,
    pub path: String,
    #[serde(default)]
    /// Demos listed using `ApiClient::list` will not have any players set, use `get_players` to automatically
    /// load the players when not set
    pub players: Option<Vec<Player>>,
}

impl Demo {
    /// Return either the stored players info or get the players from the api
    #[instrument]
    pub async fn get_players(&self, client: &ApiClient) -> Result<Cow<'_, [Player]>, Error> {
        match &self.players {
            Some(players) => Ok(Cow::Borrowed(players.as_slice())),
            None => {
                let demo = client.get(self.id).await?;
                Ok(Cow::Owned(demo.players.unwrap_or_default()))
            }
        }
    }

    /// Download a demo, returning a stream of chunks
    #[instrument]
    pub async fn download(
        &self,
        client: &ApiClient,
    ) -> Result<impl Stream<Item = Result<Bytes, Error>>, Error> {
        debug!(id = self.id, url = display(&self.url), "starting download");
        Ok(client
            .download_demo(&self.url, self.duration)
            .await?
            .bytes_stream()
            .map(|chunk| chunk.map_err(Error::from)))
    }

    /// Download a demo and save it to a writer, verifying the md5 hash in the process
    #[instrument(skip(target))]
    pub async fn save<W: Write>(&self, client: &ApiClient, mut target: W) -> Result<(), Error> {
        debug!(id = self.id, url = display(&self.url), "starting download");
        let mut response = client.download_demo(&self.url, self.duration).await?;

        let mut context = Context::new();

        while let Some(chunk) = response.chunk().await? {
            context.consume(&chunk);
            target.write_all(&chunk).map_err(Error::Write)?;
        }

        let calculated = context.compute().0;

        if calculated != self.hash {
            error!(
                calculated = display(hex::encode(calculated)),
                expected = display(hex::encode(self.hash)),
                "hash mismatch"
            );
            return Err(Error::HashMisMatch);
        }
        Ok(())
    }
}

/// Reference to a user, either contains the full user information or only the user id
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum UserRef {
    User(User),
    Id(u32),
}

impl UserRef {
    /// Id of the user
    #[must_use]
    pub fn id(&self) -> u32 {
        match self {
            UserRef::Id(id) | UserRef::User(User { id, .. }) => *id,
        }
    }

    /// Return the stored user info if available
    #[must_use]
    pub fn user(&self) -> Option<&User> {
        match self {
            UserRef::Id(_) => None,
            UserRef::User(ref user) => Some(user),
        }
    }

    /// Return either the stored user info or get the user information from the api
    #[instrument]
    pub async fn resolve(&self, client: &ApiClient) -> Result<Cow<'_, User>, Error> {
        match self {
            UserRef::User(ref user) => Ok(Cow::Borrowed(user)),
            UserRef::Id(id) => Ok(Cow::Owned(client.get_user(*id).await?)),
        }
    }
}

/// User data
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u32,
    #[serde(rename = "steamid")]
    pub steam_id: SteamID,
    pub name: String,
}

/// Data of a player in a demo
#[derive(Clone, Debug, Deserialize)]
pub struct Player {
    #[serde(rename = "id")]
    pub player_id: u32,
    #[serde(flatten)]
    #[serde(deserialize_with = "deserialize_nested_user")]
    pub user: User,
    pub team: Team,
    /// If a player has played multiple classes, the class which the user spawned the most as is taken
    pub class: Class,
    pub kills: u8,
    pub assists: u8,
    pub deaths: u8,
}

#[derive(Clone, Debug, Deserialize)]
struct NestedPlayerUser {
    user_id: u32,
    #[serde(rename = "steamid")]
    steam_id: SteamID,
    name: String,
}

fn deserialize_nested_user<'de, D>(deserializer: D) -> Result<User, D::Error>
where
    D: Deserializer<'de>,
{
    let nested = NestedPlayerUser::deserialize(deserializer)?;
    Ok(User {
        id: nested.user_id,
        steam_id: nested.steam_id,
        name: nested.name,
    })
}

/// Player team, red or blue
#[derive(Clone, Copy, Debug, Deserialize, PartialOrd, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Red,
    Blue,
}

/// Player class
#[derive(Clone, Copy, Debug, Deserialize, PartialOrd, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    Scout,
    Soldier,
    Pyro,
    Demoman,
    HeavyWeapons,
    Engineer,
    Medic,
    Sniper,
    Spy,
}

/// Deserializes a lowercase hex string to a `[u8; 16]`.
fn hex_to_digest<'de, D>(deserializer: D) -> Result<[u8; 16], D::Error>
where
    D: Deserializer<'de>,
{
    use hex::FromHex;
    use serde::de::Error;

    let string = <&str>::deserialize(deserializer)?;

    if string.is_empty() {
        return Ok([0; 16]);
    }

    <[u8; 16]>::from_hex(string).map_err(|err| Error::custom(err.to_string()))
}

/// Chat message send in the demo
#[derive(Clone, Debug, Deserialize)]
pub struct ChatMessage {
    pub user: String,
    pub time: u32,
    pub message: String,
}

/// Order for listing demos
#[derive(Debug, Clone, Copy, Serialize, Default)]
#[serde(into = "&str")]
pub enum ListOrder {
    Ascending,
    #[default]
    Descending,
}

/// Game type as recognized by demos.tf, HL, Prolander, 6s or 4v4
#[derive(Debug, Clone, Copy, Serialize)]
pub enum GameType {
    #[serde(rename = "hl")]
    HL,
    #[serde(rename = "prolander")]
    Prolander,
    #[serde(rename = "6v6")]
    Sixes,
    #[serde(rename = "4v4")]
    Fours,
}

impl Display for ListOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(<&str>::from(*self), f)
    }
}

impl From<ListOrder> for &str {
    fn from(order: ListOrder) -> Self {
        match order {
            ListOrder::Ascending => "ASC",
            ListOrder::Descending => "DESC",
        }
    }
}

/// Parameters for demo list command
#[derive(Debug, Default, Serialize)]
pub struct ListParams {
    order: ListOrder,
    backend: Option<String>,
    map: Option<String>,
    players: PlayerList,
    #[serde(rename = "type")]
    ty: Option<GameType>,
    #[serde(serialize_with = "serialize_option_time")]
    after: Option<OffsetDateTime>,
    #[serde(serialize_with = "serialize_option_time")]
    before: Option<OffsetDateTime>,
    before_id: Option<u64>,
    after_id: Option<u64>,
}

fn serialize_option_time<S>(dt: &Option<OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match dt {
        Some(time) => time::serde::timestamp::serialize(time, serializer),
        None => Option::<i64>::serialize(&None, serializer),
    }
}

#[derive(Default, Debug)]
struct PlayerList(TinyVec<[SteamID; 2]>);

impl PlayerList {
    fn new<T: Into<SteamID>, I: IntoIterator<Item = T>>(players: I) -> Self {
        PlayerList(players.into_iter().map(Into::into).collect())
    }
}

impl Display for PlayerList {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for steam_id in &self.0 {
            if first {
                first = false;
                write!(f, "{}", u64::from(*steam_id))?;
            } else {
                write!(f, ",{}", u64::from(*steam_id))?;
            }
        }

        Ok(())
    }
}

impl Serialize for PlayerList {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self)
    }
}

#[test]
fn test_serialize_player_list() {
    assert_eq!(
        "76561198024494988",
        PlayerList::new([76561198024494988]).to_string()
    );
    assert_eq!(
        "76561198024494988,76561197963701107",
        PlayerList::new([76561198024494988, 76561197963701107]).to_string()
    );
    assert_eq!(
        "76561198024494988,76561197963701107,76561197963701106",
        PlayerList::new([76561198024494988, 76561197963701107, 76561197963701106]).to_string()
    );
}

impl ListParams {
    /// Specify the backend name to filter demos with
    #[must_use]
    pub fn with_backend(self, backend: impl Into<String>) -> Self {
        ListParams {
            backend: Some(backend.into()),
            ..self
        }
    }

    /// Specify the map name to filter demos with
    #[must_use]
    pub fn with_map(self, map: impl Into<String>) -> Self {
        ListParams {
            map: Some(map.into()),
            ..self
        }
    }

    /// Specify the player steam ids to filter demos with
    #[must_use]
    pub fn with_players<T: Into<SteamID>, I: IntoIterator<Item = T>>(self, players: I) -> Self {
        ListParams {
            players: PlayerList::new(players),
            ..self
        }
    }

    /// Specify the game type to filter demos with
    #[must_use]
    pub fn with_type(self, ty: GameType) -> Self {
        ListParams {
            ty: Some(ty),
            ..self
        }
    }

    /// Specify the before date to filter demos with
    #[must_use]
    pub fn with_before(self, before: OffsetDateTime) -> Self {
        ListParams {
            before: Some(before),
            ..self
        }
    }

    /// Specify the after date to filter demos with
    #[must_use]
    pub fn with_after(self, after: OffsetDateTime) -> Self {
        ListParams {
            after: Some(after),
            ..self
        }
    }

    /// Specify the maximum demo id to filter demos with
    #[must_use]
    pub fn with_before_id(self, before: u64) -> Self {
        ListParams {
            before_id: Some(before),
            ..self
        }
    }

    /// Specify the minimum demo id to filter demos with
    #[must_use]
    pub fn with_after_id(self, after: u64) -> Self {
        ListParams {
            after_id: Some(after),
            ..self
        }
    }

    /// Specify the sort
    #[must_use]
    pub fn with_order(self, order: ListOrder) -> Self {
        ListParams { order, ..self }
    }
}
