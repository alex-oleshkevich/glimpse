use crate::{commands, extensions::Extension};
use numbat::{
    Context, InterpreterResult, module_importer::BuiltinModuleImporter, pretty_print::PrettyPrint,
    resolver::CodeSource,
};

#[derive(Clone)]
pub struct NumbatContext {
    context: Context,
}

impl NumbatContext {
    pub fn new() -> NumbatContext {
        let mut context = Context::new(BuiltinModuleImporter::default());
        context.load_currency_module_on_demand(true);
        Context::prefetch_exchange_rates();

        let _ = context.interpret("use prelude", CodeSource::Internal);
        NumbatContext { context }
    }
}

pub struct Calculator {
    context: NumbatContext,
}

impl Calculator {
    pub fn new() -> Self {
        Self {
            context: NumbatContext::new(),
        }
    }

    fn handle_query(&self, query: &String) -> Vec<crate::commands::Command> {
        if !query.starts_with("= ") {
            return vec![];
        }

        let expression = query.trim_start_matches("= ").trim();
        if let Ok(result) = self.run_numbat(expression.to_string()) {
            return vec![commands::Command::new(
                result.to_string(),
                "Press enter to copy the result to clipboard".to_string(),
                "org.gnome.Calculator".to_string(),
                vec![commands::Action::CopyToClipboard(result.to_string())],
            )];
        }
        vec![]
    }

    fn run_numbat(&self, input: String) -> Result<String, String> {
        let mut context = self.context.context.clone();
        match context.interpret(&input, CodeSource::Text) {
            Ok((statements, result)) => {
                if statements.is_empty() {
                    return Err("No statements to evaluate".to_string());
                }

                let value = match result {
                    InterpreterResult::Value(value) => format!("{}", value.pretty_print()),
                    InterpreterResult::Continue => String::from("numbat returned Continue"),
                };

                Ok(value)
            }
            Err(e) => {
                return Err(format!("Error interpreting input: {}", e));
            }
        }
    }
}

impl Extension for Calculator {
    fn id(&self) -> String {
        "me.aresa.glimpse.calculator".to_string()
    }

    fn metadata(&self) -> crate::extensions::ExtensionMetadata {
        crate::extensions::ExtensionMetadata {
            name: "Calculator".to_string(),
            description: "A simple calculator extension".to_string(),
            version: "0.1.0".to_string(),
            author: "Alex Oleshkevich".to_string(),
        }
    }

    fn query(&self, query: &crate::messages::Message) -> Vec<crate::commands::Command> {
        match query {
            crate::messages::Message::Query(query) => self.handle_query(query),
            _ => vec![],
        }
    }
}
