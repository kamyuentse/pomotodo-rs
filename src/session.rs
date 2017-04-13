extern crate serde_json;

use std::io;
use std::io::Read;
use std::sync::{Arc, Mutex};

use hyper::client::Client;
use hyper::method::Method;
use hyper::header::{Authorization, ContentType, Headers};
use hyper::status::StatusCode;
use hyper::net::HttpsConnector;
use hyper_native_tls::NativeTlsClient;
use hyper::Error as hyper_error;
use hyper::Result as hyper_result;

use uuid::Uuid;
use chrono::prelude::*;

use ::pomo::Pomo;
use ::account::Account;
use ::todo::{Todo, SubTodo};

const TODO_URL: &'static str = "https://api.pomotodo.com/1/todos";
const POMO_URL: &'static str = "https://api.pomotodo.com/1/pomos";
const ACCOUNT_URL: &'static str = "https://api.pomotodo.com/1/account";

#[derive(Debug)]
pub struct Session {
    client: Arc<Mutex<Client>>,
    token: String,
    account: Option<Account>,
}

impl Session {
    pub fn with_token(token: &str) -> hyper_result<Session> {
        let ssl = NativeTlsClient::new().unwrap();
        let connector = HttpsConnector::new(ssl);
        let client = Client::with_connector(connector);
        let token = token.to_string();

        let mut headers = Headers::new();
        headers.set(Authorization(format!("token {}", token).as_str().to_owned()));

        client.get(ACCOUNT_URL)
            .headers(headers)
            .send()
            .and_then(|mut resp| {
                if resp.status == StatusCode::Ok {
                    let mut body = String::new();
                    match resp.read_to_string(&mut body) {
                        Ok(_) => {
                            Ok(Session {
                                client: Arc::new(Mutex::new(client)),
                                token: token,
                                account: Some(serde_json::from_str(body.as_str()).unwrap()),
                            })
                        }
                        Err(e) => Err(hyper_error::Io(e)),
                    }
                } else {
                    Err(hyper_error::Status)
                }
            })
    }

    pub fn get_account(&self) -> Option<Account> {
        self.account.clone()
    }

    pub fn get_pomo(&self,
                    uuid: Option<Uuid>,
                    manual: bool,
                    abandoned: bool)
                    -> hyper_result<Vec<Pomo>> {
        let params = format!("manual={}&abandoned={}", manual, abandoned);
        let url = match uuid {
            Some(uuid) => format!("{}/{}?{}", POMO_URL, uuid, params),
            None => format!("{}?{}", POMO_URL, params),
        };

        self.get_response(url, Method::Get, None).and_then(|resp| {
            if resp.0 == StatusCode::Ok {
                Ok(serde_json::from_str::<Vec<Pomo>>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn create_pomo(&self, pomo: &Pomo) -> hyper_result<Pomo> {
        let url = POMO_URL.to_string();
        let body = pomo.to_string();

        self.get_response(url, Method::Post, Some(body)).and_then(|resp| {
            if resp.0 == StatusCode::Created {
                Ok(serde_json::from_str::<Pomo>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn update_pomo(&self, uuid: &Uuid, description: String) -> hyper_result<Pomo> {
        let url = format!("{}/{}", POMO_URL, uuid);
        let body = format!("{{ \"description\": \"{}\"}}", description);

        self.get_response(url, Method::Patch, Some(body)).and_then(|resp| {
            if resp.0 == StatusCode::Ok {
                Ok(serde_json::from_str::<Pomo>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn delete_pomo(&self, uuid: &Uuid) -> hyper_result<()> {
        let url = format!("{}/{}", POMO_URL, uuid);

        self.get_response(url, Method::Delete, None).and_then(|resp| {
            if resp.0 == StatusCode::NoContent {
                Ok(())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn get_todo(&self,
                    uuid: Option<Uuid>,
                    completed: Option<bool>,
                    completed_later_than: Option<DateTime<UTC>>,
                    completed_earlier_than: Option<DateTime<UTC>>)
                    -> hyper_result<Vec<Todo>> {
        let mut params = String::new();
        if let Some(val) = completed {
            params.push_str(format!("&completed={}", val).as_str());
        }

        if let Some(val) = completed_later_than {
            params.push_str(format!("&completed_later_than={:?}", val).as_str());
        }

        if let Some(val) = completed_earlier_than {
            params.push_str(format!("&completed_earlier_than={:?}", val).as_str());
        }

        let mut url = match uuid {
            Some(uuid) => format!("{}/{}", TODO_URL, uuid),
            None => TODO_URL.to_string(),
        };

        if !params.is_empty() {
            params.remove(0);
            params.insert(0, '?');
            url.push_str(params.as_str());
        }

        self.get_response(url, Method::Get, None).and_then(|resp| {
            if resp.0 == StatusCode::Ok {
                Ok(serde_json::from_str::<Vec<Todo>>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn create_todo(&self, todo: &Todo) -> hyper_result<Todo> {
        let url = TODO_URL.to_string();

        let body = serde_json::to_string(todo).unwrap();

        self.get_response(url, Method::Post, Some(body)).and_then(|resp| {
            if resp.0 == StatusCode::Created {
                Ok(serde_json::from_str::<Todo>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn update_todo(&self, todo: &Todo) -> hyper_result<Todo> {
        if todo.uuid.is_none() {
            return Err(hyper_error::from(io::Error::new(io::ErrorKind::InvalidData,
                                                        "Uuid is null.")));
        }
        let url = format!("{}/{:}", TODO_URL, todo.uuid.unwrap());

        // Set not allowed fields to `None`
        let mut todo = todo.clone();
        todo.uuid = None;
        todo.created_at = None;
        todo.updated_at = None;

        let body = serde_json::to_string(&todo).unwrap();
        self.get_response(url, Method::Patch, Some(body)).and_then(|resp| {
            if resp.0 == StatusCode::Ok {
                Ok(serde_json::from_str::<Todo>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn delete_todo(&self, uuid: &Uuid) -> hyper_result<()> {
        let url = format!("{}/{:}", TODO_URL, uuid);

        self.get_response(url, Method::Delete, None).and_then(|resp| {
            if resp.0 == StatusCode::NoContent {
                Ok(())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn get_subtodo(&self, parent_id: &Uuid) -> hyper_result<Vec<SubTodo>> {
        let url = format!("{}/{}/sub_todos", TODO_URL, parent_id);

        self.get_response(url, Method::Get, None).and_then(|resp| {
            if resp.0 == StatusCode::Ok {
                Ok(serde_json::from_str::<Vec<SubTodo>>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn create_subtodo(&self, parent_id: &Uuid, sub_todo: &SubTodo) -> hyper_result<SubTodo> {
        let url = format!("{}/{}/sub_todos", TODO_URL, parent_id);

        let body = serde_json::to_string(sub_todo).unwrap();

        self.get_response(url, Method::Post, Some(body)).and_then(|resp| {
            if resp.0 == StatusCode::Created {
                Ok(serde_json::from_str::<SubTodo>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn update_subtodo(&self, parent_id: &Uuid, sub_todo: &SubTodo) -> hyper_result<SubTodo> {
        if sub_todo.uuid.is_none() {
            return Err(hyper_error::from(io::Error::new(io::ErrorKind::InvalidData,
                                                        "Uuid is null.")));
        }

        let url = format!("{}/{}/sub_todos/{}",
                          TODO_URL,
                          parent_id,
                          sub_todo.uuid.unwrap());

        // Set not allowed fields to `None`
        let mut sub_todo = sub_todo.clone();
        sub_todo.uuid = None;
        sub_todo.created_at = None;
        sub_todo.updated_at = None;

        let body = serde_json::to_string(&sub_todo).unwrap();

        self.get_response(url, Method::Patch, Some(body)).and_then(|resp| {
            if resp.0 == StatusCode::Ok {
                Ok(serde_json::from_str::<SubTodo>(resp.1.as_str()).unwrap())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    pub fn delete_subtodo(&self, parent_id: &Uuid, uuid: &Uuid) -> hyper_result<()> {
        let url = format!("{}/{}/sub_todos/{}", TODO_URL, parent_id, uuid);

        self.get_response(url, Method::Delete, None).and_then(|resp| {
            if resp.0 == StatusCode::NoContent {
                Ok(())
            } else {
                Err(hyper_error::Status)
            }
        })
    }

    fn get_response(&self,
                    url: String,
                    method: Method,
                    body: Option<String>)
                    -> hyper_result<(StatusCode, String)> {
        let client = self.client.lock().unwrap();
        let mut headers = Headers::new();
        headers.set(Authorization(format!("token {}", self.token).as_str().to_owned()));

        if body.is_some() {
            headers.set(ContentType::json());
        }

        if let Some(body) = body {
            client.request(method, url.as_str())
                .headers(headers)
                .body(body.as_str())
                .send()
                .and_then(|mut resp| {
                    let mut resp_body = String::new();
                    match resp.read_to_string(&mut resp_body) {
                        Ok(_) => Ok((resp.status, resp_body)),
                        Err(e) => Err(hyper_error::Io(e)),
                    }
                })
        } else {
            client.request(method, url.as_str())
                .headers(headers)
                .send()
                .and_then(|mut resp| {
                    let mut resp_body = String::new();
                    match resp.read_to_string(&mut resp_body) {
                        Ok(_) => Ok((resp.status, resp_body)),
                        Err(e) => Err(hyper_error::Io(e)),
                    }
                })
        }
    }
}