use std::sync::Arc;

#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, serde::Serialize)]
pub enum Body {
    Json(serde_json::Value),
    Raw(String),
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, serde::Serialize)]
pub struct RecordedRequestBody {
    pub method: String,
    pub path_and_query: String,
    pub body: Option<Body>,
}

impl From<&mockito::Request> for RecordedRequestBody {
    fn from(value: &mockito::Request) -> Self {
        Self {
            method: value.method().to_string(),
            path_and_query: value.path_and_query().to_string(),
            body: match value.header("content-type").first() {
                Some(h) if h == &"application/json" => {
                    Some(Body::Json(serde_json::from_slice(value.body().unwrap()).unwrap()))
                }
                _ => Some(Body::Raw(value.utf8_lossy_body().unwrap().to_string())),
            },
        }
    }
}

pub struct RequestRecorder {
    requests: Arc<parking_lot::Mutex<Vec<RecordedRequestBody>>>,
}

impl std::fmt::Debug for RequestRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestRecorder")
            .field("requests", &self.requests.lock())
            .finish()
    }
}

impl PartialEq for RequestRecorder {
    fn eq(&self, other: &Self) -> bool {
        let a = self.requests.lock();
        let b = other.requests.lock();

        a.as_slice().eq(b.as_slice())
    }
}

impl Default for RequestRecorder {
    fn default() -> Self {
        Self {
            requests: Arc::new(parking_lot::Mutex::new(Vec::with_capacity(2))),
        }
    }
}

impl RequestRecorder {
    pub fn as_matcher(&self) -> impl Fn(&mockito::Request) -> bool + use<> {
        let requests = self.requests.clone();
        move |req| {
            requests.lock().push(RecordedRequestBody::from(req));
            true
        }
    }

    pub fn requests(self) -> Vec<RecordedRequestBody> {
        self.requests.lock().clone()
    }
}
