use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rand::{distr::Alphanumeric, RngExt};
use reqwest::{
    blocking::{Client, Response},
    Method, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::app_state::TickTickToken;

const AUTH_URL: &str = "https://ticktick.com/oauth/authorize";
const TOKEN_URL: &str = "https://ticktick.com/oauth/token";
const API_BASE_URL: &str = "https://api.ticktick.com";
const OAUTH_SCOPE: &str = "tasks:write tasks:read";
const TOKEN_KEY: &[u8] = b"TodoCli_TickTickToken_XOR_Key_2026";
const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:56610/callback";

fn token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".todo_ticktick_token")
}

pub fn load_token() -> Option<TickTickToken> {
    let raw = fs::read(token_path()).ok()?;
    let decrypted = xor(&raw);
    serde_json::from_slice::<TickTickToken>(&decrypted).ok()
}

pub fn save_token(token: &TickTickToken) -> Result<(), String> {
    let raw = serde_json::to_vec(token).map_err(|e| e.to_string())?;
    let encrypted = xor(&raw);
    let path = token_path();
    fs::write(&path, encrypted).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn xor(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ TOKEN_KEY[i % TOKEN_KEY.len()])
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn open_browser(url: &str) -> Result<(), String> {
    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd").arg("/C").arg("start").arg(url).status()
    } else {
        Command::new("xdg-open").arg(url).status()
    };

    result.map_err(|e| e.to_string()).and_then(|status| {
        if status.success() {
            Ok(())
        } else {
            Err("failed to open browser".into())
        }
    })
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}

pub fn oauth_connect(client_id: &str, client_secret: &str) -> Result<TickTickToken, String> {
    let redirect_uri = env::var("TICKTICK_REDIRECT_URI")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REDIRECT_URI.to_string());
    let parsed_redirect = Url::parse(&redirect_uri).map_err(|e| e.to_string())?;
    let host = parsed_redirect
        .host_str()
        .ok_or_else(|| "redirect uri must include host".to_string())?;
    let port = parsed_redirect
        .port_or_known_default()
        .ok_or_else(|| "redirect uri must include port".to_string())?;
    let listener_addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&listener_addr)
        .map_err(|e| format!("cannot bind callback listener on {listener_addr}: {e}"))?;
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let state: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    let auth_url = Url::parse_with_params(
        AUTH_URL,
        &[
            ("scope", OAUTH_SCOPE),
            ("client_id", client_id),
            ("state", &state),
            ("redirect_uri", &redirect_uri),
            ("response_type", "code"),
        ],
    )
    .map_err(|e| e.to_string())?;

    println!("Opening TickTick login in your browser...");
    println!();
    println!("If browser does not open, use this URL manually:");
    println!("{}", auth_url.as_str());
    println!();
    println!("Registered redirect URI must match exactly: {redirect_uri}");
    if let Err(e) = open_browser(auth_url.as_str()) {
        println!("Could not auto-open browser: {e}");
    }

    println!("Waiting for OAuth callback on {redirect_uri}");
    let (code, returned_state) = wait_for_code(listener)?;
    if returned_state != state {
        return Err("OAuth state mismatch, aborting login".into());
    }

    let http = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let res = http
        .post(TOKEN_URL)
        .basic_auth(client_id, Some(client_secret))
        .form(&[
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("scope", OAUTH_SCOPE),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .map_err(|e| format!("token exchange failed: {e}"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().unwrap_or_default();
        return Err(format!("token exchange failed ({status}): {body}"));
    }

    let token: OAuthTokenResponse = res.json().map_err(|e| e.to_string())?;
    Ok(TickTickToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at_unix: token.expires_in.map(|v| now_unix() + v.saturating_sub(30)),
        scope: token.scope,
    })
}

fn wait_for_code(listener: TcpListener) -> Result<(String, String), String> {
    listener
        .set_ttl(64)
        .map_err(|e| format!("listener setup failed: {e}"))?;
    listener
        .set_nonblocking(false)
        .map_err(|e| format!("listener setup failed: {e}"))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    while std::time::Instant::now() < deadline {
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        let mut buf = [0_u8; 4096];
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let first = req.lines().next().unwrap_or_default().to_string();
        let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
        let req_url = Url::parse(&format!("http://localhost{path}")).map_err(|e| e.to_string())?;
        let query: std::collections::HashMap<_, _> = req_url.query_pairs().into_owned().collect();
        let code = query.get("code").cloned().unwrap_or_default();
        let state = query.get("state").cloned().unwrap_or_default();

        let msg = if code.is_empty() {
            "OAuth failed. You can close this tab and retry."
        } else {
            "TickTick connected. You can close this tab and return to terminal."
        };
        let body = format!("<html><body><h3>{msg}</h3></body></html>");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();

        if !code.is_empty() {
            return Ok((code, state));
        }
    }

    Err("timed out waiting for TickTick OAuth callback".into())
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TickTickProject {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TickTickTask {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub status: i32,
}

#[derive(Deserialize)]
struct TickTickProjectData {
    #[serde(default)]
    tasks: Vec<TickTickTask>,
}

pub struct TickTickApi {
    http: Client,
    client_id: String,
    client_secret: String,
    token: TickTickToken,
}

impl TickTickApi {
    pub fn new(
        client_id: String,
        client_secret: String,
        token: TickTickToken,
    ) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            client_id,
            client_secret,
            token,
        })
    }

    pub fn token(&self) -> &TickTickToken {
        &self.token
    }

    fn is_expired(&self) -> bool {
        self.token
            .expires_at_unix
            .map(|ts| ts <= now_unix())
            .unwrap_or(false)
    }

    fn refresh_if_needed(&mut self) -> Result<(), String> {
        if !self.is_expired() {
            return Ok(());
        }
        self.refresh_token()
    }

    fn refresh_token(&mut self) -> Result<(), String> {
        let refresh_token = self
            .token
            .refresh_token
            .clone()
            .ok_or_else(|| "missing refresh token, reconnect required".to_string())?;

        let res = self
            .http
            .post(TOKEN_URL)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .map_err(|e| format!("refresh token request failed: {e}"))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().unwrap_or_default();
            return Err(format!("refresh token failed ({status}): {body}"));
        }

        let token: OAuthTokenResponse = res.json().map_err(|e| e.to_string())?;
        self.token.access_token = token.access_token;
        if token.refresh_token.is_some() {
            self.token.refresh_token = token.refresh_token;
        }
        self.token.expires_at_unix = token.expires_in.map(|v| now_unix() + v.saturating_sub(30));
        self.token.scope = token.scope;
        Ok(())
    }

    fn send_json(
        &mut self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Response, String> {
        self.refresh_if_needed()?;
        let url = format!("{API_BASE_URL}{path}");

        for _ in 0..2 {
            let mut req = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&self.token.access_token);
            if let Some(payload) = &body {
                req = req.json(payload);
            }
            let resp = req.send().map_err(|e| e.to_string())?;
            if resp.status() == StatusCode::UNAUTHORIZED {
                self.refresh_token()?;
                continue;
            }
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().unwrap_or_default();
                return Err(format!("TickTick API error ({status}) on {path}: {body}"));
            }
            return Ok(resp);
        }

        Err("request failed after token refresh".into())
    }

    pub fn list_projects(&mut self) -> Result<Vec<TickTickProject>, String> {
        let resp = self.send_json(Method::GET, "/open/v1/project", None)?;
        resp.json::<Vec<TickTickProject>>()
            .map_err(|e| e.to_string())
    }

    pub fn create_project(&mut self, name: &str) -> Result<TickTickProject, String> {
        let payload = json!({
            "name": name,
            "viewMode": "list",
            "kind": "TASK",
        });
        let resp = self.send_json(Method::POST, "/open/v1/project", Some(payload))?;
        resp.json::<TickTickProject>().map_err(|e| e.to_string())
    }

    pub fn project_tasks(&mut self, project_id: &str) -> Result<Vec<TickTickTask>, String> {
        let path = format!("/open/v1/project/{project_id}/data");
        let resp = self.send_json(Method::GET, &path, None)?;
        let data: TickTickProjectData = resp.json().map_err(|e| e.to_string())?;
        Ok(data.tasks)
    }

    pub fn create_task(
        &mut self,
        project_id: &str,
        title: &str,
        desc: &str,
        done: bool,
    ) -> Result<TickTickTask, String> {
        let status = if done { 2 } else { 0 };
        let payload = json!({
            "projectId": project_id,
            "title": title,
            "desc": desc,
            "status": status
        });
        let resp = self.send_json(Method::POST, "/open/v1/task", Some(payload))?;
        resp.json::<TickTickTask>().map_err(|e| e.to_string())
    }

    pub fn update_task(
        &mut self,
        project_id: &str,
        task_id: &str,
        title: &str,
        desc: &str,
        done: bool,
    ) -> Result<(), String> {
        let status = if done { 2 } else { 0 };
        let payload = json!({
            "id": task_id,
            "projectId": project_id,
            "title": title,
            "desc": desc,
            "status": status
        });
        let path = format!("/open/v1/task/{task_id}");
        self.send_json(Method::POST, &path, Some(payload))
            .map(|_| ())
    }

    pub fn complete_task(&mut self, project_id: &str, task_id: &str) -> Result<(), String> {
        let path = format!("/open/v1/project/{project_id}/task/{task_id}/complete");
        self.send_json(Method::POST, &path, None).map(|_| ())
    }

    pub fn delete_task(&mut self, project_id: &str, task_id: &str) -> Result<(), String> {
        let path = format!("/open/v1/project/{project_id}/task/{task_id}");
        self.send_json(Method::DELETE, &path, None).map(|_| ())
    }
}
