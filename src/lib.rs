use reqwest::{multipart, Client, IntoUrl, StatusCode, Url};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;
pub use steamid_ng::SteamID;
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid base url: {0}")]
    InvalidBaseUrl(reqwest::Error),
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Invalid page requested")]
    InvalidPage,
    #[error("Invalid api key")]
    InvalidApiKey,
    #[error("Hash mismatch")]
    HashMisMatch,
    #[error("Unknown server error")]
    ServerError(u16),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Demo {0} not found")]
    DemoNotFound(u32),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// Demos listed using `ApiClient::list` will not have any players set
    pub players: Vec<Player>,
}

impl Demo {
    /// Return either the stored players info or get the players from the api
    pub async fn get_players<'a>(&'a self, client: &ApiClient) -> Result<Cow<'a, [Player]>, Error> {
        if !self.players.is_empty() {
            Ok(Cow::Borrowed(self.players.as_slice()))
        } else {
            let demo = client.get(self.id).await?;
            Ok(Cow::Owned(demo.players))
        }
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
    pub fn id(&self) -> u32 {
        match self {
            UserRef::Id(id) => *id,
            UserRef::User(User { id, .. }) => *id,
        }
    }

    /// Return the stored user info if available
    pub fn user(&self) -> Option<&User> {
        match self {
            UserRef::Id(_) => None,
            UserRef::User(ref user) => Some(user),
        }
    }

    /// Return either the stored user info or get the user information from the api
    pub async fn resolve<'a>(&'a self, client: &ApiClient) -> Result<Cow<'a, User>, Error> {
        match self {
            UserRef::User(ref user) => Ok(Cow::Borrowed(user)),
            UserRef::Id(id) => Ok(Cow::Owned(client.get_user(*id).await?)),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u32,
    #[serde(rename = "steamid")]
    pub steam_id: SteamID,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Player {
    #[serde(rename = "id")]
    pub player_id: u32,
    #[serde(flatten)]
    #[serde(deserialize_with = "deserialize_nested_user")]
    pub user: User,
    pub team: Team,
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

#[derive(Clone, Copy, Debug, Deserialize, PartialOrd, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Red,
    Blue,
}

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

#[derive(Clone, Debug, Deserialize)]
pub struct ChatMessage {
    pub user: String,
    pub time: u32,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(into = "&str")]
pub enum ListOrder {
    Ascending,
    Descending,
}

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

impl Default for ListOrder {
    fn default() -> Self {
        ListOrder::Descending
    }
}

impl fmt::Display for ListOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <&str>::from(*self).fmt(f)
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
struct PlayerList(Vec<SteamID>);

impl Serialize for PlayerList {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        self.0
            .iter()
            .map(|steamid| format!("{}", u64::from(*steamid)))
            .collect::<Vec<_>>()
            .join(",")
            .serialize(serializer)
    }
}

impl ListParams {
    pub fn with_backend(self, backend: impl ToString) -> Self {
        ListParams {
            backend: Some(backend.to_string()),
            ..self
        }
    }

    pub fn with_map(self, map: impl ToString) -> Self {
        ListParams {
            map: Some(map.to_string()),
            ..self
        }
    }

    pub fn with_players<T: Into<SteamID>, I: IntoIterator<Item = T>>(self, players: I) -> Self {
        ListParams {
            players: PlayerList(players.into_iter().map(Into::into).collect()),
            ..self
        }
    }

    pub fn with_type(self, ty: GameType) -> Self {
        ListParams {
            ty: Some(ty),
            ..self
        }
    }

    pub fn with_before(self, before: OffsetDateTime) -> Self {
        ListParams {
            before: Some(before),
            ..self
        }
    }

    pub fn with_after(self, after: OffsetDateTime) -> Self {
        ListParams {
            after: Some(after),
            ..self
        }
    }

    pub fn with_order(self, order: ListOrder) -> Self {
        ListParams { order, ..self }
    }
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: Url,
}

impl Default for ApiClient {
    fn default() -> Self {
        ApiClient::new()
    }
}

/// Api client for demos.tf
///
/// # Example
///
/// ```rust
/// use demostf_client::{ListOrder, ListParams, ApiClient};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), demostf_client::Error> {
/// let client = ApiClient::new();
///
/// let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await?;
///
/// for demo in demos {
///     println!("{}: {}", demo.id, demo.name);
/// }
/// # Ok(())
/// # }
/// ```
impl ApiClient {
    const DEMOS_TF_BASE_URL: &'static str = "https://api.demos.tf";

    /// Create an api client for the default demos.tf endpoint
    pub fn new() -> Self {
        ApiClient::with_base_url(ApiClient::DEMOS_TF_BASE_URL).unwrap()
    }

    /// Create an api client using a different api endpoint
    pub fn with_base_url(base_url: impl IntoUrl) -> Result<Self, Error> {
        Ok(ApiClient {
            client: Client::new(),
            base_url: base_url.into_url().map_err(Error::InvalidBaseUrl)?,
        })
    }

    /// List demos with the provided options
    ///
    /// note that the pages start counting at 1
    ///
    /// # Example
    ///
    /// ```rust
    /// use demostf_client::{ListOrder, ListParams};
    /// # use demostf_client::ApiClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), demostf_client::Error> {
    /// # let client = ApiClient::default();
    /// #
    /// let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await?;
    ///
    /// for demo in demos {
    ///     println!("{}: {}", demo.id, demo.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(&self, params: ListParams, page: u32) -> Result<Vec<Demo>, Error> {
        if page == 0 {
            return Err(Error::InvalidPage);
        }

        let mut url = self.base_url.clone();
        url.set_path("/demos");
        Ok(self
            .client
            .get(url)
            .query(&[("page", page)])
            .query(&params)
            .send()
            .await?
            .json()
            .await?)
    }

    /// List demos uploaded by a user with the provided options
    ///
    /// note that the pages start counting at 1
    ///
    /// # Example
    ///
    /// ```rust
    /// use demostf_client::{ListOrder, ListParams};
    /// # use demostf_client::ApiClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), demostf_client::Error> {
    /// # use steamid_ng::SteamID;
    /// let client = ApiClient::default();
    /// #
    /// let demos = client.list_uploads(SteamID::from(76561198024494988), ListParams::default().with_order(ListOrder::Ascending), 1).await?;
    ///
    /// for demo in demos {
    ///     println!("{}: {}", demo.id, demo.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_uploads(
        &self,
        uploader: SteamID,
        params: ListParams,
        page: u32,
    ) -> Result<Vec<Demo>, Error> {
        if page == 0 {
            return Err(Error::InvalidPage);
        }

        let mut url = self.base_url.clone();
        url.set_path(&format!("/uploads/{}", u64::from(uploader)));
        Ok(self
            .client
            .get(url)
            .query(&[("page", page)])
            .query(&params)
            .send()
            .await?
            .json()
            .await?)
    }

    /// Get the data for a single demo
    ///
    /// # Example
    ///
    /// ```rust
    /// # use demostf_client::ApiClient;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), demostf_client::Error> {
    /// # let client = ApiClient::default();
    /// #
    /// let demo = client.get(9).await?;
    ///
    /// println!("{}: {}", demo.id, demo.name);
    /// println!("players:");
    ///
    /// for player in demo.players {
    ///     println!("{}", player.user.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get(&self, demo_id: u32) -> Result<Demo, Error> {
        let mut url = self.base_url.clone();
        url.set_path(&format!("/demos/{}", demo_id));
        let response = self.client.get(url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::DemoNotFound(demo_id));
        }

        Ok(response.error_for_status()?.json().await?)
    }

    /// Get user info by id
    ///
    /// # Example
    ///
    /// ```rust
    /// # use demostf_client::ApiClient;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), demostf_client::Error> {
    /// # let client = ApiClient::default();
    /// #
    /// let user = client.get_user(1).await?;
    ///
    /// println!("{} ({})", user.name, user.steam_id.steam3());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_user(&self, user_id: u32) -> Result<User, Error> {
        let mut url = self.base_url.clone();
        url.set_path(&format!("/users/{}", user_id));
        Ok(self.client.get(url).send().await?.json().await?)
    }

    /// List demos with the provided options
    ///
    /// # Example
    ///
    /// ```rust
    /// # use demostf_client::ApiClient;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), demostf_client::Error> {
    /// # let client = ApiClient::default();
    /// #
    /// let chat = client.get_chat(447678).await?;
    ///
    /// for message in chat {
    ///     println!("{}: {}", message.user, message.message);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_chat(&self, demo_id: u32) -> Result<Vec<ChatMessage>, Error> {
        let mut url = self.base_url.clone();
        url.set_path(&format!("/demos/{}/chat", demo_id));
        Ok(self.client.get(url).send().await?.json().await?)
    }

    pub async fn set_url(
        &self,
        demo_id: u32,
        backend: &str,
        path: &str,
        url: &str,
        hash: [u8; 16],
        key: &str,
    ) -> Result<(), Error> {
        let mut api_url = self.base_url.clone();
        api_url.set_path(&format!("/demos/{}/url", demo_id));

        let respose = self
            .client
            .post(api_url)
            .form(&[
                ("hash", hex::encode(hash).as_str()),
                ("backend", backend),
                ("url", url),
                ("path", path),
                ("key", key),
            ])
            .send()
            .await?;

        match respose.status() {
            StatusCode::UNAUTHORIZED => Err(Error::InvalidApiKey),
            StatusCode::PRECONDITION_FAILED => Err(Error::HashMisMatch),
            _ if respose.status().is_server_error() => {
                Err(Error::ServerError(respose.status().as_u16()))
            }
            _ => Ok(()),
        }
    }

    pub async fn upload_demo(
        &self,
        file_name: String,
        body: Vec<u8>,
        red: String,
        blue: String,
        key: String,
    ) -> Result<u32, Error> {
        let form = multipart::Form::new()
            .text("red", red)
            .text("blue", blue)
            .text("name", file_name)
            .text("key", key);

        let file = multipart::Part::bytes(body)
            .file_name("demo.dem")
            .mime_str("text/plain")?;

        let form = form.part("demo", file);

        let resp = self
            .client
            .post(self.base_url.join("/upload").unwrap())
            .multipart(form)
            .send()
            .await?
            .text()
            .await?;

        if resp == "Invalid key" {
            return Err(Error::InvalidApiKey);
        }

        let tail = resp.split('/').last().unwrap_or_default();
        u32::from_str(tail).map_err(|_| Error::InvalidResponse(resp))
    }
}
