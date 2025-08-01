use std::env;
use std::path::Path;
use std::process;

use glimpse_sdk::{Action, Command, Icon, SearchPlugin, ReplyWriter, Response};
use numbat::markup::{Formatter, PlainTextFormatter};
use numbat::module_importer::BuiltinModuleImporter;
use numbat::pretty_print::PrettyPrint;
use numbat::resolver::CodeSource;
use numbat::{Context, InterpreterResult};

struct CalculatorPlugin {
    context: Context,
}

impl CalculatorPlugin {
    fn new() -> Self {
        let mut context = Context::new(BuiltinModuleImporter::default());
        let _ = context.interpret("use prelude", CodeSource::Internal);
        context.load_currency_module_on_demand(true);
        Context::prefetch_exchange_rates();
        Self { context }
    }
}

impl SearchPlugin for CalculatorPlugin {
    async fn search(&self, query: String, output: &mut ReplyWriter<'_>) {
        if !query.starts_with("=") {
            return;
        }

        let input = query.trim_start_matches('=').trim();
        if input.is_empty() {
            return;
        }
        let mut context = self.context.clone();
        let result = context.interpret(input, CodeSource::Text);
        if result.is_err() {
            tracing::debug!(
                "error interpreting input: {}",
                result.as_ref().err().unwrap()
            );
            return;
        }


        let (_, result) = result.unwrap();
        let formatter = PlainTextFormatter;
        let value = match result {
            InterpreterResult::Value(value) => value,
            InterpreterResult::Continue => {
                tracing::debug!("interpreter returned Continue, no value to display");
                return;
            }
        };

        tracing::debug!("calculated value: {}", &value);
        tracing::debug!("formatted calculated value: {}", formatter.format(&value.pretty_print(), false));
        output
            .reply(Response::SearchResults(vec![Command {
                title: input.to_string(),
                subtitle: value.pretty_print().to_string(),
                icon: Icon::Freedesktop {
                    name: "calculator".to_string(),
                },
                category: "Calculator".to_string(),
                actions: vec![Action::CopyToClipboard {
                    text: value.pretty_print().to_string(),
                }],
            }]))
            .await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path>", args[0]);
        process::exit(1);
    }

    let socket_path = &args[1];
    if !Path::new(socket_path).exists() {
        process::exit(1);
    }

    let plugin = CalculatorPlugin::new();
    if let Err(e) = plugin.run(socket_path.into()).await {
        eprintln!("Error running plugin: {}", e);
        process::exit(1);
    }

    Ok(())
}
