use std::error::Error;

use async_trait::async_trait;
use freedesktop_icons::lookup;
use glimpse_sdk::{Match, Metadata, Plugin, PluginError, run_plugin, setup_logging};
use numbat::{
    Context, InterpreterResult,
    markup::{Formatter, PlainTextFormatter},
    module_importer::BuiltinModuleImporter,
    pretty_print::PrettyPrint,
    resolver::CodeSource,
};

struct App {
    context: Context,
    icon: String,
}

impl App {
    fn new() -> Self {
        let possible_icons = vec![
            "calculator",
            "calculator-symbolic",
            "accessories-calculator",
            "accessories-calculator-symbolic",
            "system-calculator",
            "system-calculator-symbolic",
            "gnome-calculator",
            "calculator-app",
            "org.gnome.Calculator",
            "org.kde.plasma.calculator",
            "applications-science",
            "applications-science-symbolic",
            "application-x-executable",
        ];
        let theme_file_icon = possible_icons
            .iter()
            .find_map(|icon_name| {
                lookup(icon_name)
                    .with_size(64)
                    .find()
                    .map(|f| f.to_string_lossy().to_string())
            })
            .or_else(|| {
                tracing::warn!("no icon found in theme, using fallback icon");
                Some("application-x-executable".to_string())
            })
            .unwrap();

        tracing::debug!("using icon: {}", theme_file_icon);
        let mut context = Context::new(BuiltinModuleImporter::default());
        let _ = context.interpret("use prelude", CodeSource::Internal);
        context.load_currency_module_on_demand(true);
        Context::prefetch_exchange_rates();
        Self {
            context,
            icon: theme_file_icon,
        }
    }
}

#[async_trait]
impl Plugin for App {
    fn metadata(&self) -> Metadata {
        Metadata {
            id: "me.aresa.glimpse.calculator".to_string(),
            name: "Calculator Plugin".to_string(),
            version: "0.1.1".to_string(),
            description: "A simple calculator plugin that performs basic arithmetic operations and unit conversions."
                .to_string(),
            author: "Alex Oleshkevich <alex.oleshkevich@gmail.com>".to_string(),
        }
    }

    async fn search(&self, query: String) -> Result<Vec<Match>, PluginError> {
        if !query.starts_with("=") {
            tracing::debug!("query does not start with '=', ignoring");
            return Ok(vec![]);
        }

        let input = query.trim_start_matches('=').trim();
        if input.is_empty() {
            tracing::debug!(
                "input is empty after trimming '=', ignoring, query: '{}'",
                query
            );
            return Ok(vec![]);
        }

        let mut context = self.context.clone();
        let result = context.interpret(input, CodeSource::Text);
        if result.is_err() {
            tracing::debug!(
                "error interpreting input: {}",
                result.as_ref().err().unwrap()
            );
            return Ok(vec![]);
        }

        let (_, result) = result.unwrap();
        let formatter = PlainTextFormatter;
        let value = match result {
            InterpreterResult::Value(value) => value,
            InterpreterResult::Continue => {
                tracing::debug!("interpreter returned Continue, no value to display");
                return Ok(vec![]);
            }
        };

        tracing::debug!("calculated value: {}", &value);
        tracing::debug!(
            "formatted calculated value: {}",
            formatter.format(&value.pretty_print(), false)
        );
        Ok(vec![Match {
            title: input.to_string(),
            description: value.pretty_print().to_string(),
            icon: Some(self.icon.clone()),
            score: 1.0,
            actions: vec![glimpse_sdk::MatchAction {
                title: "Copy to Clipboard".to_string(),
                close_on_action: true,
                action: glimpse_sdk::Action::Clipboard {
                    text: value.pretty_print().to_string(),
                },
            }],
        }])
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    setup_logging(tracing::Level::DEBUG);
    let plugin = App::new();
    if let Err(err) = run_plugin(plugin).await {
        tracing::error!("error running plugin: {}", err);
    }
    Ok(())
}
