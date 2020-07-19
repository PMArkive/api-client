use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use reqwest::{Client, IntoUrl, Url};
use thiserror::Error;
use steamid_ng::SteamID;
use std::borrow::Cow;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid base url: {0}")]
    InvalidBaseUrl(#[source] reqwest::Error),
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Invalid page requested")]
    InvalidPage,
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
    #[serde(with = "chrono::serde::ts_seconds")]
    pub time: DateTime<Utc>,
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
        if self.players.len() > 0 {
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
            UserRef::User(User { id, .. }) => *id
        }
    }

    /// Return the stored user info if available
    pub fn user(&self) -> Option<&User> {
        match self {
            UserRef::Id(_) => None,
            UserRef::User(ref user) => Some(user)
        }
    }

    /// Return either the stored user info or get the user information from the api
    pub async fn resolve<'a>(&'a self, client: &ApiClient) -> Result<Cow<'a, User>, Error> {
        match self {
            UserRef::User(ref user) => Ok(Cow::Borrowed(user)),
            UserRef::Id(id) => Ok(Cow::Owned(client.get_user(*id).await?))
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u32,
    #[serde(rename = "steamid")]
    pub steam_id: SteamID,
    pub name: String,
    pub avatar: String,
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
    avatar: String,
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
        avatar: nested.avatar,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Red,
    Blue,
}

#[derive(Clone, Debug, Deserialize)]
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

    if string.len() == 0 {
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
}

impl ListParams {
    pub fn with_backend(self, backend: impl ToString) -> Self {
        ListParams {
            backend: Some(backend.to_string()),
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
        Ok(self.client.get(url)
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
        Ok(self.client.get(url)
            .send()
            .await?
            .json()
            .await?)
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
        Ok(self.client.get(url)
            .send()
            .await?
            .json()
            .await?)
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
        Ok(self.client.get(url)
            .send()
            .await?
            .json()
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{ApiClient, ListParams, ListOrder};
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
}