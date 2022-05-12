use crate::{ChatMessage, Demo, Error, ListParams, User};
use reqwest::{multipart, Client, IntoUrl, Response, StatusCode, Url};
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
}

impl Default for ApiClient {
    fn default() -> Self {
        ApiClient::new()
    }
}

impl Debug for ApiClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiClient")
            .field("base_url", &self.base_url.to_string())
            .finish_non_exhaustive()
    }
}

impl ApiClient {
    pub const DEMOS_TF_BASE_URL: &'static str = "https://api.demos.tf";

    /// Create an api client for the default demos.tf endpoint
    pub fn new() -> Self {
        ApiClient::with_base_url(ApiClient::DEMOS_TF_BASE_URL).unwrap()
    }

    /// Create an api client using a different api endpoint
    pub fn with_base_url(base_url: impl IntoUrl) -> Result<Self, Error> {
        ApiClient::with_base_url_and_timeout(base_url, Duration::from_secs(15))
    }

    /// Create an api client using a different api endpoint
    pub fn with_base_url_and_timeout(
        base_url: impl IntoUrl,
        timeout: Duration,
    ) -> Result<Self, Error> {
        Ok(ApiClient {
            base_timeout: timeout,
            client: Client::builder().timeout(timeout).build()?,
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
    #[instrument]
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
            .error_for_status()?
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
    #[instrument]
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
    /// for player in demo.players {
    ///     println!("{}", player.user.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument]
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
    #[instrument]
    pub async fn get_user(&self, user_id: u32) -> Result<User, Error> {
        let mut url = self.base_url.clone();
        url.set_path(&format!("/users/{}", user_id));
        Ok(self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
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
    #[instrument]
    pub async fn get_chat(&self, demo_id: u32) -> Result<Vec<ChatMessage>, Error> {
        let mut url = self.base_url.clone();
        url.set_path(&format!("/demos/{}/chat", demo_id));
        Ok(self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
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
        let mut api_url = self.base_url.clone();
        api_url.set_path(&format!("/demos/{}/url", demo_id));

        self.client
            .post(api_url)
            .form(&[
                ("hash", hex::encode(hash).as_str()),
                ("backend", backend),
                ("url", url),
                ("path", path),
                ("key", key),
            ])
            .send()
            .await?
            .error_for_status()?;

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
            .post(self.base_url.join("/upload").unwrap())
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
        let timeout_scale = (duration as f32 / 60.0).max(15.0) / 15.0;
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
