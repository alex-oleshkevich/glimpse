use glob::glob;
use std::process::Command;

fn compile_blueprints() {
    let blueprint_files: Vec<String> = glob("resources/ui/*.blp")
        .expect("Failed to read glob pattern")
        .filter_map(|entry| entry.ok())
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    for blueprint_file in &blueprint_files {
        let output_file = blueprint_file.replace(".blp", ".ui");

        println!("cargo:rerun-if-changed={}", blueprint_file);

        let output = Command::new("blueprint-compiler")
            .arg("compile")
            .arg("--output")
            .arg(&output_file)
            .arg(blueprint_file)
            .output();

        match output {
            Ok(output) => {
                if !output.status.success() {
                    eprintln!(
                        "Blueprint compilation failed for {}: {}",
                        blueprint_file,
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(_) => {
                eprintln!(
                    "blueprint-compiler not found, skipping Blueprint compilation for {}",
                    blueprint_file
                );
                eprintln!("Install with: sudo apt install blueprint-compiler");
            }
        }
    }
}

fn main() {
    compile_blueprints();
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/resources.gresource.xml",
        "resources.gresource",
    );
}
