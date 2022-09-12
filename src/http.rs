use bytes::Buf;
use core::time::Duration;
use reqwest::{IntoUrl, Url};
use std::io::Read;

pub type Result<T> = core::result::Result<T, Error>;

pub struct ClientBuilder {
    builder: reqwest::blocking::ClientBuilder,
}

impl ClientBuilder {
    fn new() -> Self {
        Self {
            builder: reqwest::blocking::Client::builder()
                .user_agent("nixos-lxc-generator/v0.0.1")
                .referer(false)
                .use_rustls_tls()
                .https_only(true),
        }
    }

    pub fn connect_timeout<T: Into<Option<Duration>>>(mut self, timeout: T) -> Self {
        self.builder = self.builder.connect_timeout(timeout);

        self
    }

    pub fn request_timeout<T: Into<Option<Duration>>>(mut self, timeout: T) -> Self {
        self.builder = self.builder.timeout(timeout);

        self
    }

    pub fn build(self) -> Result<Client> {
        Client::new(self.builder.build()?)
    }
}

pub struct Client {
    client: reqwest::blocking::Client,
}

impl Client {
    fn new(client: reqwest::blocking::Client) -> Result<Self> {
        Ok(Self { client })
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub fn get(&self, req: GetRequest) -> Result<Response> {
        let resp = self.client.get(req.url).send()?;

        if !resp.status().is_success() {
            return Err(Error::new(format!("HTTP error: {}", resp.status())));
        }

        Ok(Response { inner: resp })
    }
}

pub struct GetRequest {
    url: Url,
}

impl GetRequest {
    pub fn new(u: impl IntoUrl) -> Result<Self> {
        let url = u.into_url()?;

        Ok(Self { url })
    }
}

pub struct Response {
    inner: reqwest::blocking::Response,
}

impl Response {
    pub fn as_text(self) -> Result<String> {
        Ok(self.inner.text()?)
    }

    pub fn as_reader(self) -> Result<impl Read> {
        Ok(self.inner.bytes()?.reader())
    }
}

#[derive(Debug)]
pub struct Error {
    error: String,
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Error::new(format!("{}", error))
    }
}

impl Error {
    fn new(error: String) -> Self {
        Self { error }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}
