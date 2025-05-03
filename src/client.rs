use crate::{ChatMessage, Demo, Error, ListParams, User};
use reqwest::{multipart, Client, IntoUrl, Response, StatusCode, Url};
use std::borrow::Borrow;
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;
use std::time::Duration;
use steamid_ng::SteamID;
use tracing::{instrument, trace};

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
#[derive(Clone)]
pub struct ApiClient {
    base_timeout: Duration,
    client: Client,
    base_url: Url,
    access_key: Option<String>,
}

impl Default for ApiClient {
    fn default() -> Self {
        ApiClient::new()
    }
}

impl Debug for ApiClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiClient")
            .field("base_url", &format_args!("{}", self.base_url))
            .finish_non_exhaustive()
    }
}

impl ApiClient {
    pub const DEMOS_TF_BASE_URL: &'static str = "https://api.demos.tf/";

    /// Create an api client for the default demos.tf endpoint
    #[must_use]
    pub fn new() -> Self {
        ApiClient::with_base_url(ApiClient::DEMOS_TF_BASE_URL).unwrap_or_else(|_| unreachable!())
    }

    /// Create an api client using a different api endpoint
    ///
    /// # Errors
    ///
    /// Returns an error when the provided `base_url` is not a valid url
    pub fn with_base_url(base_url: impl IntoUrl) -> Result<Self, Error> {
        ApiClient::with_base_url_and_timeout(base_url, Duration::from_secs(15))
    }

    /// Create an api client using a different api endpoint and timeout
    ///
    /// # Errors
    ///
    /// Returns an error when the provided `base_url` is not a valid url
    pub fn with_base_url_and_timeout(
        base_url: impl IntoUrl,
        timeout: Duration,
    ) -> Result<Self, Error> {
        // ensure there is always a leading / to prevent unexpected behavior with url creation later
        let mut base_url = base_url.into_url().map_err(|_| Error::InvalidBaseUrl)?;
        if !base_url.path().ends_with("/") {
            base_url.set_path(&format!("{}/", base_url.path()));
        }

        Ok(ApiClient {
            base_timeout: timeout,
            client: Client::builder().timeout(timeout).build()?,
            base_url,
            access_key: None,
        })
    }

    /// Set access key used to access private demos
    pub fn set_access_key(&mut self, access_key: String) {
        self.access_key = Some(access_key);
    }

    fn url<P: AsRef<str>>(&self, path: P) -> Result<Url, Error> {
        self.base_url
            .join(path.as_ref())
            .map_err(|_| Error::InvalidBaseUrl)
    }

    fn url_with_params<P, I, K, V>(&self, path: P, iter: I) -> Result<Url, Error>
    where
        P: AsRef<str>,
        I: IntoIterator,
        I::Item: Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut url = self
            .base_url
            .join(path.as_ref())
            .map_err(|_| Error::InvalidBaseUrl)?;
        url.query_pairs_mut().extend_pairs(iter);
        Ok(url)
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
    #[instrument]
    pub async fn list(&self, params: ListParams, page: u32) -> Result<Vec<Demo>, Error> {
        self.list_url(self.url("demos")?, params, page).await
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
    #[instrument]
    pub async fn list_uploads(
        &self,
        uploader: SteamID,
        params: ListParams,
        page: u32,
    ) -> Result<Vec<Demo>, Error> {
        self.list_url(
            self.url(format!("uploads/{}", u64::from(uploader)))?,
            params,
            page,
        )
        .await
    }

    async fn list_url(&self, url: Url, params: ListParams, page: u32) -> Result<Vec<Demo>, Error> {
        if page == 0 {
            return Err(Error::InvalidPage);
        }

        let mut req = self.client.get(url);

        if let Some(access_key) = &self.access_key {
            req = req.header("ACCESS_KEY", access_key.as_str());
        }

        Ok(req
            .query(&[("page", page)])
            .query(&params)
            .send()
            .await?
            .error_for_status()?
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
    /// for player in demo.players.unwrap_or_default() {
    ///     println!("{}", player.user.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument]
    pub async fn get(&self, demo_id: u32) -> Result<Demo, Error> {
        let mut req = self.client.get(self.url(format!("/demos/{}", demo_id))?);

        if let Some(access_key) = &self.access_key {
            req = req.header("ACCESS-KEY", access_key.as_str());
        }

        let response = req.send().await?;

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
    #[instrument]
    pub async fn get_user(&self, user_id: u32) -> Result<User, Error> {
        let response = self
            .client
            .get(self.url(format!("/users/{}", user_id))?)
            .send()
            .await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::UserNotFound(user_id));
        }

        Ok(response.error_for_status()?.json().await?)
    }

    /// Search for players by name
    ///
    /// # Example
    ///
    /// ```rust
    /// # use demostf_client::ApiClient;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), demostf_client::Error> {
    /// let client = ApiClient::default();
    /// #
    /// let users = client.search_users("icewind").await?;
    ///
    /// for user in users {
    ///   println!("{} ({})", user.name, user.steam_id.steam3());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument]
    pub async fn search_users(&self, name: &str) -> Result<Vec<User>, Error> {
        let response = self
            .client
            .get(self.url_with_params("/users/search", [("query", name)])?)
            .send()
            .await?;

        Ok(response.error_for_status()?.json().await?)
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
    #[instrument]
    pub async fn get_chat(&self, demo_id: u32) -> Result<Vec<ChatMessage>, Error> {
        let response = self
            .client
            .get(self.url(format!("/demos/{}/chat", demo_id))?)
            .send()
            .await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::DemoNotFound(demo_id));
        }

        Ok(response.error_for_status()?.json().await?)
    }

    #[instrument]
    pub async fn set_url(
        &self,
        demo_id: u32,
        backend: &str,
        path: &str,
        url: &str,
        hash: [u8; 16],
        key: &str,
    ) -> Result<(), Error> {
        let response = self
            .client
            .post(self.url(format!("/demos/{}/url", demo_id))?)
            .form(&[
                ("hash", hex::encode(hash).as_str()),
                ("backend", backend),
                ("url", url),
                ("path", path),
                ("key", key),
            ])
            .send()
            .await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::DemoNotFound(demo_id));
        }

        response.error_for_status()?;

        Ok(())
    }

    #[instrument(skip(body))]
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
            .post(self.url("/upload")?)
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        if resp == "Invalid key" {
            return Err(Error::InvalidApiKey);
        }

        let tail = resp.split('/').last().unwrap_or_default();
        u32::from_str(tail).map_err(|_| Error::InvalidResponse(resp))
    }

    pub(crate) async fn download_demo(&self, url: &str, duration: u16) -> Result<Response, Error> {
        // set timeout to 1s per 60s (~1mb) with a minimum of 15s, scaled by an configured timeout (default 15s)
        let timeout_scale = (f32::from(duration) / 60.0).max(15.0) / 15.0;
        let timeout = Duration::from_secs_f32(self.base_timeout.as_secs_f32() * timeout_scale);
        trace!(url = url, timeout = debug(timeout), "requesting demo file");
        Ok(self
            .client
            .get(url)
            .timeout(timeout)
            .send()
            .await?
            .error_for_status()?)
    }
}

#[test]
fn test_url() {
    assert_eq!(
        "https://example.com/demos",
        ApiClient::with_base_url("https://example.com")
            .unwrap()
            .url("demos")
            .unwrap()
            .to_string()
    );
    assert_eq!(
        "https://example.com/demos",
        ApiClient::with_base_url("https://example.com/")
            .unwrap()
            .url("demos")
            .unwrap()
            .to_string()
    );
    assert_eq!(
        "https://example.com/sub/demos",
        ApiClient::with_base_url("https://example.com/sub/")
            .unwrap()
            .url("demos")
            .unwrap()
            .to_string()
    );
    assert_eq!(
        "https://example.com/sub/demos",
        ApiClient::with_base_url("https://example.com/sub")
            .unwrap()
            .url("demos")
            .unwrap()
            .to_string()
    );
}
