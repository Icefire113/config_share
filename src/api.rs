use std::io::Read;

use api_types::{
    create_file::{CreateFileReq, CreateFileRes},
    create_token::{CreateTokenReq, CreateTokenRes},
    create_user::{CreateUserReq, CreateUserRes},
    get_file::{GetAllFilesReq, GetAllFilesRes, GetFileContentRes, GetFileInfoRes},
    get_token_info::{GetTokenInfoReq, GetTokenInfoRes},
    get_user::GetUserRes,
    internal_err::InternalErrorRes,
    request_token::{RequestTokenReq, RequestTokenRes},
    revoke_token::RevokeTokenReq,
    update_file::{UpdateFileReq, UpdateFileRes},
    update_user::{UpdateUserReq, UpdateUserRes},
};
use serde::de::DeserializeOwned;
use ureq::{RequestBuilder, http::Response};
use url::Url;
use uuid::Uuid;

type ApiResponse = Response<ureq::Body>;

#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    #[error("transport error: {0}")]
    Transport(#[from] ureq::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("api error [{status}] {code}: {detail:?}")]
    Api {
        status: u16,
        code: String,
        detail: Option<String>,
    },
}

/// A client for the sync_server API, built from `openapi.yaml`.
#[derive(Debug, Clone)]
pub struct ApiClient {
    agent: ureq::Agent,
    base_url: Url,
    token: Option<String>,
}

impl ApiClient {
    /// Creates a new client pointed at the given server url.
    pub fn new(base_url: Url) -> Self {
        // Disable ureq's default behaviour of turning 4xx/5xx into errors so that
        // we can read and parse the `InternalErrorRes` body ourselves.
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build(),
        );

        Self {
            agent,
            base_url,
            token: None,
        }
    }

    /// Sets the bearer token used to authenticate requests.
    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }

    fn url(&self, path: &str) -> String {
        self.base_url
            .join(path)
            .expect("valid API path")
            .to_string()
    }

    fn apply_auth<B>(&self, req: RequestBuilder<B>) -> RequestBuilder<B> {
        if let Some(token) = &self.token {
            req.header("Authorization", format!("Bearer {}", token))
        } else {
            req
        }
    }

    /// Parses a response into the expected typed response, or an `Api` error if the
    /// status is not a success.
    fn parse<R: DeserializeOwned>(
        &self,
        res: ApiResponse,
        expect: u16,
    ) -> Result<R, ApiClientError> {
        let status = res.status().as_u16();
        if status != expect {
            return Err(self.parse_error(res));
        }
        let mut body = res.into_body();
        Ok(body.read_json()?)
    }

    /// Parses an error response body into an `ApiClientError::Api`.
    fn parse_error(&self, res: ApiResponse) -> ApiClientError {
        let status = res.status().as_u16();
        let mut body = res.into_body();
        let err: InternalErrorRes = body.read_json().unwrap_or(InternalErrorRes {
            code: String::new(),
            error_detail: None,
        });

        ApiClientError::Api {
            status,
            code: err.code,
            detail: err.error_detail,
        }
    }
}

// Version + healthcheck
impl ApiClient {
    /// GET /version the running server version string.
    pub fn version(&self) -> Result<String, ApiClientError> {
        let req = self.agent.get(self.url("/version"));
        let res = self.apply_auth(req).call()?;
        let status = res.status().as_u16();
        if status != 200 {
            return Err(self.parse_error(res));
        }
        let mut body = String::new();
        res.into_body().as_reader().read_to_string(&mut body)?;
        Ok(body)
    }
}

// Files
impl ApiClient {
    /// GET /api/files lists all tracked files for the authenticated user.
    pub fn get_all_files(&self, req: &GetAllFilesReq) -> Result<GetAllFilesRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.get(self.url("/api/files")))
            .query("include_deleted", req.include_deleted.to_string())
            .query("limit", req.limit.unwrap_or(100).to_string())
            .query("offset", req.offset.unwrap_or(0).to_string())
            .call()?;
        self.parse(res, 200)
    }

    /// POST /api/files uploads a new tracked file.
    pub fn create_file(&self, req: &CreateFileReq) -> Result<CreateFileRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.post(self.url("/api/files")))
            .send_json(req)?;
        self.parse(res, 201)
    }

    /// GET /api/files/{id} downloads the raw file contents.
    pub fn get_file_contents(&self, id: &Uuid) -> Result<GetFileContentRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.get(self.url(&format!("/api/files/{}", id))))
            .call()?;
        let status = res.status().as_u16();
        if status != 200 {
            return Err(self.parse_error(res));
        }
        Ok(res.into_body().read_to_vec()?)
    }

    /// DELETE /api/files/{id} soft-deletes a tracked file.
    pub fn delete_file(&self, id: &Uuid) -> Result<(), ApiClientError> {
        let res = self
            .apply_auth(self.agent.delete(self.url(&format!("/api/files/{}", id))))
            .call()?;
        let status = res.status().as_u16();
        if status != 204 {
            return Err(self.parse_error(res));
        }
        Ok(())
    }

    /// PATCH /api/files/{id} updates a tracked file's name and/or contents.
    pub fn update_file(
        &self,
        id: &Uuid,
        req: &UpdateFileReq,
    ) -> Result<UpdateFileRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.patch(self.url(&format!("/api/files/{}", id))))
            .send_json(req)?;
        self.parse(res, 201)
    }

    /// GET /api/files/{id}/info fetches a file's metadata without its contents.
    pub fn get_file_info(&self, id: &Uuid) -> Result<GetFileInfoRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.get(self.url(&format!("/api/files/{}/info", id))))
            .call()?;
        self.parse(res, 200)
    }
}

// Auth tokens
impl ApiClient {
    /// POST /api/auth/tokens/create creates a new access token.
    pub fn create_token(&self, req: &CreateTokenReq) -> Result<CreateTokenRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.post(self.url("/api/auth/tokens/create")))
            .send_json(req)?;
        self.parse(res, 201)
    }

    /// GET /api/auth/tokens/info inspects an access token.
    pub fn token_info(&self, req: &GetTokenInfoReq) -> Result<GetTokenInfoRes, ApiClientError> {
        let res = self
            .apply_auth(
                self.agent
                    .get(self.url("/api/auth/tokens/info"))
                    .query("token", &req.token),
            )
            .call()?;
        self.parse(res, 200)
    }

    /// POST /api/auth/tokens/request requests a token from username + password.
    pub fn request_token(&self, req: &RequestTokenReq) -> Result<RequestTokenRes, ApiClientError> {
        let res = self
            .agent
            .post(self.url("/api/auth/tokens/request"))
            .send_json(req)?;
        self.parse(res, 201)
    }

    /// PATCH /api/auth/tokens/revoke revokes an access token by id.
    pub fn revoke_token(&self, req: &RevokeTokenReq) -> Result<(), ApiClientError> {
        let res = self
            .apply_auth(self.agent.patch(self.url("/api/auth/tokens/revoke")))
            .send_json(req)?;
        let status = res.status().as_u16();
        if status != 204 {
            return Err(self.parse_error(res));
        }
        Ok(())
    }
}

// Auth users
impl ApiClient {
    /// POST /api/auth/users/create creates a new user.
    pub fn create_user(&self, req: &CreateUserReq) -> Result<CreateUserRes, ApiClientError> {
        let res = self
            .agent
            .post(self.url("/api/auth/users/create"))
            .send_json(req)?;
        self.parse(res, 201)
    }

    /// GET /api/auth/users/{id} fetches a user's information (admin).
    pub fn get_user(&self, id: i64) -> Result<GetUserRes, ApiClientError> {
        let res = self
            .apply_auth(self.agent.get(self.url(&format!("/api/auth/users/{}", id))))
            .call()?;
        self.parse(res, 200)
    }

    /// PATCH /api/auth/users/{id} updates a user's enabled state and/or role (admin).
    pub fn update_user(
        &self,
        id: i64,
        req: &UpdateUserReq,
    ) -> Result<UpdateUserRes, ApiClientError> {
        let res = self
            .apply_auth(
                self.agent
                    .patch(self.url(&format!("/api/auth/users/{}", id))),
            )
            .send_json(req)?;
        self.parse(res, 200)
    }

    /// DELETE /api/auth/users/{id} deletes a user (admin).
    pub fn delete_user(&self, id: i64) -> Result<(), ApiClientError> {
        let res = self
            .apply_auth(
                self.agent
                    .delete(self.url(&format!("/api/auth/users/{}", id))),
            )
            .call()?;
        let status = res.status().as_u16();
        if status != 204 {
            return Err(self.parse_error(res));
        }
        Ok(())
    }
}
