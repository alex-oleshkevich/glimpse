# {{name}}

Applet id: `{{id}}`. Source directory: `{{dir}}`.

Run `deno task check` to typecheck, then `glimpsectl applets dev .` to link it to a running panel. Add `center = ["{{id}}"]` to a panel's applet zone. Run `glimpsectl applets bundle .` to build a package tree or `glimpsectl applets install .` for a user install.
