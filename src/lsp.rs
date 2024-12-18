use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tower_lsp::{LspService, Server};

pub async fn run_server() {
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| Lsp {
        client,
        state: Mutex::new(LspState {
            config: Config {
                api_key: None,
                project: None,
            },
            sources: HashMap::new(),

            members: HashSet::new(),
            labels: HashSet::new(),
            milestones: HashSet::new(),
        }),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

pub struct LspState {
    pub config: Config,

    // see https://github.com/ebkalderon/nix-language-server/blob/master/src/backend.rs#L14-L23
    /// Mapping of path names to file contents.
    pub sources: HashMap<String, String>,

    labels: HashSet<CompletionItemData>,
    members: HashSet<CompletionItemData>,
    milestones: HashSet<CompletionItemData>,
}

#[derive(Debug)]
pub struct Config {
    pub api_key: Option<String>,
    pub project: Option<String>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct CompletionItemData {
    completion: String,
    description: Option<String>,
}

enum Resource {
    Labels,
    Members,
    Milestones,
    QuickActions,
}

pub struct Lsp {
    pub client: Client,
    pub state: Mutex<LspState>,
}

macro_rules! log {
    // log!(self, LEVEL, "format {args} and {}", such)
    // where level is LOG, INFO, WARNING, ERROR
    ($self:ident, $lvl:ident, $($arg:tt)+) => ({
        $self.client
            .log_message(MessageType::$lvl, format!($($arg)+))
            .await;
    });

    // log!(self, "format {args} and {}", such)
    ($self:ident, $($arg:tt)+) => ({
        $self.client
            .log_message(MessageType::LOG, format!($($arg)+))
            .await;
    });
}

macro_rules! log_debug {
    // log!(self, LEVEL, "format {args} and {}", such)
    // where level is LOG, INFO, WARNING, ERROR
    ($self:ident, $lvl:ident, $($arg:tt)+) => ({
        #[cfg(debug_assertions)]
        $self.client
            .log_message(MessageType::$lvl, format!($($arg)+))
            .await;
    });

    // log!(self, "format {args} and {}", such)
    ($self:ident, $($arg:tt)+) => ({
        #[cfg(debug_assertions)]
        $self.client
            .log_message(MessageType::LOG, format!($($arg)+))
            .await;
    });
}

#[tower_lsp::async_trait]
impl LanguageServer for Lsp {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        log!(
            self,
            "[initialize] initializing {} {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        );
        log_debug!(self, "[initialize] {params:?}");

        let mut state = self.state.lock().await;

        match std::env::var_os("GITLAB_API_PRIVATE_TOKEN") {
            Some(token) => state.config.api_key = Some(token.to_string_lossy().to_string()),
            None => {
                return Err(Error {
                    code: ErrorCode::ServerError(1),
                    message: "Error: no GITLAB_API_PRIVATE_TOKEN environment variable detected"
                        .into(),
                    data: None,
                })
            }
        };

        if let Some(ref opts) = params.initialization_options {
            match opts.get("project") {
                Some(Value::String(project)) => {
                    state.config.project = Some(project.clone());
                }
                Some(_) => {
                    return Err(Error {
                        code: ErrorCode::ServerError(1),
                        message:
                            "Error: invalid configuration param 'project' supplied, expected string"
                                .into(),
                        data: None,
                    })
                }
                None => {
                    return Err(Error {
                        code: ErrorCode::ServerError(1),
                        message: "Error: required configuration param 'project' not supplied"
                            .into(),
                        data: None,
                    })
                }
            }
        }
        // log_debug!(self, "[initialize:config] {:#?}", state.config);

        let api_base = "https://gitlab.com/api/v4";
        let project = state.config.project.clone().unwrap();
        let api_key = state.config.api_key.clone().unwrap();
        let verbose = true;
        let client = reqwest::ClientBuilder::new()
            .connection_verbose(verbose)
            .build()
            .expect("TODO");

        let requests = vec![
            make_request(&client, api_base, &api_key, &project, Resource::Labels),
            make_request(&client, api_base, &api_key, &project, Resource::Milestones),
            make_request(&client, api_base, &api_key, &project, Resource::Members),
        ];
        let responses = futures::future::join_all(requests).await;
        for res in responses {
            match res {
                Ok((resource_kind, Value::Array(json))) => {
                    let values = process_resource(&resource_kind, json);
                    match resource_kind {
                        Resource::Labels => {
                            state.labels = values;
                        }
                        Resource::Members => {
                            state.members = values;
                        }
                        Resource::Milestones => {
                            state.milestones = values;
                        }
                        Resource::QuickActions => unreachable!(),
                    }
                }

                Ok((_, _json)) => log!(
                    self,
                    ERROR,
                    "Received unexpected or invalid JSON from Gitlab API."
                ),
                Err(err) => log!(self, ERROR, "Received response error: {err}"),
            }
        }

        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "/".to_string(),
                        "@".to_string(),
                        "%".to_string(),
                        "~".to_string(),
                    ]),
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    completion_item: None,
                }),
                execute_command_provider: None,
                workspace: None,
                // workspace: Some(WorkspaceServerCapabilities {
                //     workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                //         supported: Some(true),
                //         change_notifications: Some(OneOf::Left(true)),
                //     }),
                //     file_operations: None,
                // }),
                document_formatting_provider: None,
                // TODO go to defn of issue/MR, etc
                definition_provider: None,
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        log_debug!(self, "[initialized] {_params:?}");
    }

    async fn shutdown(&self) -> Result<()> {
        log_debug!(self, "[shutdown]");
        Ok(())
    }

    async fn did_change_workspace_folders(&self, _params: DidChangeWorkspaceFoldersParams) {
        log_debug!(self, "[did_change_workspace_folders] {_params:?}");
    }

    async fn did_change_configuration(&self, _params: DidChangeConfigurationParams) {
        log_debug!(self, "[did_change_configuration] {_params:?}");
    }

    async fn did_change_watched_files(&self, _params: DidChangeWatchedFilesParams) {
        log_debug!(self, "[did_change_watched_files] {_params:?}");
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        {
            let mut p = params.clone();
            p.text_document.text = "...trimmed...".to_string();
            log_debug!(self, "[did_open] {p:?}");
        }

        let mut state = self.state.lock().await;
        state.sources.insert(
            params.text_document.uri.path().to_owned(),
            params.text_document.text.clone(),
        );
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        {
            let mut p = params.clone();
            p.content_changes = p
                .content_changes
                .into_iter()
                .map(|mut c| {
                    c.text = "...trimmed...".to_string();
                    c
                })
                .collect();
            log_debug!(self, "[did_change] {p:?}");
        }

        let mut state = self.state.lock().await;
        let content = match params.content_changes.first() {
            Some(content) => content.text.clone(),
            None => String::new(),
        };
        state
            .sources
            .insert(params.text_document.uri.path().to_owned(), content.clone());
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        {
            let mut p = params.clone();
            p.text = Some("...trimmed...".to_string());
            log_debug!(self, "[did_save] {p:?}");
        }
    }

    async fn did_close(&self, _params: DidCloseTextDocumentParams) {
        log_debug!(self, "[did_close] {_params:?}");
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        log_debug!(self, "[completion] {params:?}");

        // let contents = contents_of_path(params.text_document_position.text_document.uri.path());
        let state = self.state.lock().await;
        let pathname = params.text_document_position.text_document.uri.path();
        let contents = match state.sources.get(pathname) {
            Some(contents) => contents.clone(),
            None => return Ok(None),
        };

        // dbg!(params.text_document_position.position);

        let line = contents
            .lines()
            .nth(params.text_document_position.position.line as usize)
            .expect("line (row) should exist");
        let index = params
            .text_document_position
            .position
            .character
            .saturating_sub(1) as usize;

        let (current_word_start, current_word_end) = {
            let boundary_chars = vec![' ', '\t'];

            if let Some((line_start, line_end)) = line.split_at_checked(index) {
                log_debug!(self, "line_start: {line_start:?}");
                log_debug!(self, "line_end: {line_end:?}");

                let start_offset = line_start
                    .rfind(boundary_chars.as_slice())
                    .map_or_else(|| 0, |i| i + 1);
                let end_offset = line_end
                    .find(boundary_chars.as_slice())
                    .unwrap_or(line_end.len());

                log_debug!(self, "offset: {start_offset}..{end_offset}");

                (start_offset, index + end_offset)
            } else {
                (index, index)
            }
        };
        let ch = line
            .chars()
            .nth(current_word_start)
            .expect("char (column) should exist");

        log_debug!(self, "line: {line}");
        log_debug!(self, "ch: {ch}");

        let (completions, completion_kind) = match ch {
            '/' => (
                // https://docs.gitlab.com/ee/user/project/quick_actions.html
                // these are all aimed at creating *new* issues at this time, so
                // eg /reopen or /unassign aren't relevant
                vec![
                    ("/assign ", "Assign users"),
                    ("/blocked_by ", "Is blocked by other issues"),
                    ("/blocks ", "Blocks other issues"),
                    ("/due ", "Due on a certain date"),
                    ("/relate ", "Relates to other issues"),
                    ("/label ", "Add labels"),
                    ("/milestone ", "Add to milestone"),
                    ("/title ", "Set title"),
                ]
                .iter()
                .map(|i| CompletionItemData {
                    completion: i.0.to_string(),
                    description: Some(i.1.to_string()),
                })
                .collect::<Vec<CompletionItemData>>(),
                Resource::QuickActions,
            ),
            '@' => (state.members.iter().cloned().collect(), Resource::Members),
            '%' => (
                state.milestones.iter().cloned().collect(),
                Resource::Milestones,
            ),
            '~' => (state.labels.iter().cloned().collect(), Resource::Labels),
            _ => return Ok(None),
        };

        let detail = match completion_kind {
            Resource::Labels => "label",
            Resource::Members => "username",
            Resource::Milestones => "milestone",
            Resource::QuickActions => "quick action",
        };
        let completion_kind = match completion_kind {
            Resource::Labels | Resource::Members | Resource::Milestones => {
                Some(CompletionItemKind::CONSTANT)
            }
            Resource::QuickActions => Some(CompletionItemKind::KEYWORD),
        };
        let range = Range {
            start: Position {
                line: params.text_document_position.position.line,
                character: current_word_start as u32,
            },
            end: Position {
                line: params.text_document_position.position.line,
                character: current_word_end as u32,
            },
        };

        let completions: Vec<CompletionItem> = completions
            .iter()
            .map(|comp| {
                let mut completion =
                    CompletionItem::new_simple(comp.completion.to_string(), detail.to_string());

                completion.kind = completion_kind.clone();
                completion.documentation =
                    comp.description.clone().map(|d| Documentation::String(d));
                completion.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: comp.completion.to_string(),
                }));

                // To use a snippet
                // completion.insert_text = Some(period.snippet.clone());
                // completion.insert_text_format = Some(InsertTextFormat::SNIPPET);

                completion
            })
            .collect();

        Ok(Some(CompletionResponse::Array(completions)))
    }
}

fn gitlab_resource_url(api_base: &str, project: &str, resource_kind: &Resource) -> String {
    let api_base = api_base.strip_suffix("/").unwrap_or(api_base);
    let project = project.replace('/', "%2F");
    let resource = match resource_kind {
        Resource::Labels => "labels",
        Resource::Members => "members/all",
        Resource::Milestones => "milestones",
        Resource::QuickActions => unreachable!(),
    };
    // See: https://docs.gitlab.com/ee/api/rest/index.html#offset-based-pagination
    format!("{api_base}/projects/{project}/{resource}?per_page=100")
}

fn make_request(
    client: &reqwest::Client,
    api_base: &str,
    api_key: &str,
    project: &str,
    resource_kind: Resource,
) -> tokio::task::JoinHandle<(Resource, Value)> {
    let label_url = gitlab_resource_url(api_base, &project, &resource_kind);

    let cl = client
        .get(label_url)
        .bearer_auth(&api_key)
        .try_clone()
        .expect("Cloning client");

    tokio::spawn(async move {
        let res = cl.send().await.expect("awaiting request");
        // let pages = res
        //     .headers()
        //     .get("x-total-pages")
        //     .map_or(1, |v| v.to_str().map_or(1, |s| s.parse().unwrap_or(1)));
        let json: serde_json::Value = res.json().await.expect("decoding JSON");
        (resource_kind, json)
    })
}

fn process_resource(
    resource_kind: &Resource,
    resources: Vec<Value>,
) -> HashSet<CompletionItemData> {
    resources
        .into_iter()
        .filter_map(|r| match r {
            Value::Object(resource) => {
                // https://docs.gitlab.com/ee/api/labels.html#list-labels
                // https://docs.gitlab.com/ee/api/milestones.html
                // https://docs.gitlab.com/ee/api/members.html#list-all-members-of-a-group-or-project

                let (gitlab_prefix, value_key, description_key) = match resource_kind {
                    Resource::Labels => ("~", "name", "description"),
                    Resource::Members => ("@", "username", "name"),
                    Resource::Milestones => {
                        if let Value::Bool(true) = resource["expired"] {
                            return None;
                        }

                        ("%", "title", "description")
                    }
                    Resource::QuickActions => unreachable!(),
                };

                let (completion, description) =
                    match (&resource[value_key], &resource[description_key]) {
                        (Value::String(ref completion), Value::String(ref description))
                            if !description.is_empty() =>
                        {
                            (completion, Some(description.clone()))
                        }
                        (Value::String(ref completion), _) => (completion, None),
                        (_, _) => {
                            return None;
                        }
                    };

                let completion = if completion.contains(&[' ']) {
                    format!(r#"{gitlab_prefix}"{completion}" "#)
                } else {
                    format!("{gitlab_prefix}{completion} ")
                };

                Some(CompletionItemData {
                    completion,
                    description,
                })
            }
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Array(_) => None,
        })
        .collect()
}
