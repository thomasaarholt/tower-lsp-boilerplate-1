use std::sync::{Arc, Mutex};

use chumsky::prelude::Simple;
use chumsky::Parser;
use diagnostic_ls::chumsky::{parser, Json};
use serde_json::{Error, Value};
use std::sync::mpsc::channel;
use tokio::sync::mpsc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::Full,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["dummy.do_something".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::Info, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(MessageType::Info, "workspace folders changed!")
            .await;
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.client
            .log_message(MessageType::Info, "configuration changed!")
            .await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.client
            .log_message(MessageType::Info, "watched files have changed!")
            .await;
    }

    async fn execute_command(&self, _: ExecuteCommandParams) -> Result<Option<Value>> {
        self.client
            .log_message(MessageType::Info, "command executed!")
            .await;

        match self.client.apply_edit(WorkspaceEdit::default()).await {
            Ok(res) if res.applied => self.client.log_message(MessageType::Info, "applied").await,
            Ok(_) => self.client.log_message(MessageType::Info, "rejected").await,
            Err(err) => self.client.log_message(MessageType::Error, err).await,
        }

        Ok(None)
    }

    async fn did_open(&self, _: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::Info, "file opened!")
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let rope = ropey::Rope::from_str(&params.content_changes[0].text);
        let result = {
            let res = parser();
            let res = res.parse_recovery(std::mem::take(&mut params.content_changes[0].text));

            res.1
        };
        let diagnostics = result
            .into_iter()
            .map(|item| {
                let msg = format!(
                    "{}{}, expected {}",
                    if item.found().is_some() {
                        "Unexpected token"
                    } else {
                        "Unexpected end of input"
                    },
                    if let Some(label) = item.label() {
                        format!(" while parsing {}", label)
                    } else {
                        String::new()
                    },
                    if item.expected().len() == 0 {
                        "something else".to_string()
                    } else {
                        item.expected()
                            .map(|expected| match expected {
                                Some(expected) => expected.to_string(),
                                None => "end of input".to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                );
                let span = item.span();
                let start_line = rope.char_to_line(span.start);
                let first_char = rope.line_to_char(start_line);
                let start_column = span.start - first_char;

                let end_line = rope.char_to_line(span.end);
                let first_char = rope.line_to_char(end_line);
                let end_column = span.end - first_char;
                Diagnostic::new_simple(
                    Range::new(
                        Position::new(start_line as u32, start_column as u32),
                        Position::new(end_line as u32, end_column as u32),
                    ),
                    msg,
                )
            })
            .collect::<Vec<_>>();

        self.client
            .log_message(MessageType::Info, format!("{:?}", diagnostics))
            .await;
        self.client
            .publish_diagnostics(
                params.text_document.uri.clone(),
                diagnostics,
                Some(params.text_document.version),
            )
            .await;
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::Info, "file saved!")
            .await;
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        self.client
            .log_message(MessageType::Info, "file closed!")
            .await;
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service)
        .await;
}
