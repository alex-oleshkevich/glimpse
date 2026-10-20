{ lib, rustPlatform, pkg-config, blueprint-compiler, gettext, wrapGAppsHook4
, clang, llvmPackages, gtk4, libadwaita, gtk4-layer-shell, libheif, pam
, libpulseaudio, pipewire, systemd, udev, wayland, coreutils, glib }:
rustPlatform.buildRustPackage {
  pname = "glimpse";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;

  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type:
      let
        root = toString ../.;
        relative = lib.removePrefix "${root}/" (toString path);
        top = builtins.head (lib.splitString "/" relative);
      in
        (toString path == root || builtins.elem top [ "Cargo.toml" "Cargo.lock" "LICENSE" "crates" "data" "po" "sdk" ])
        && !(lib.hasPrefix "crates/glimpse-widgets/resources/widgets" relative)
        && relative != "po/glimpse.pot";
  };

  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [ "--workspace" "--bins" ];
  doCheck = false;

  nativeBuildInputs = [ pkg-config blueprint-compiler gettext wrapGAppsHook4 clang ];
  buildInputs = [ gtk4 libadwaita gtk4-layer-shell libheif pam libpulseaudio pipewire systemd udev wayland ];
  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

  preBuild = ''
    export PREFIX="$out"
  '';

  postInstall = ''
    install -Dm755 data/bin/glimpse-dpms "$out/bin/glimpse-dpms"
    patchShebangs "$out/bin/glimpse-dpms"

    mkdir -p "$out/share/glimpse/sdk" "$out/share/glimpse/themes"
    cp -r sdk/templates "$out/share/glimpse/sdk/"
    cp -r data/themes/. "$out/share/glimpse/themes/"
    for name in config.default.toml config.commented.toml config.schema.json language-codes.json; do
      install -Dm644 "data/$name" "$out/share/glimpse/$name"
    done
    install -Dm644 LICENSE "$out/share/glimpse/LICENSE"

    for unit in data/systemd/*.service; do
      target="$out/lib/systemd/user/$(basename "$unit")"
      install -Dm644 "$unit" "$target"
      substituteInPlace "$target" \
        --replace-fail /usr/bin/glimpse "$out/bin/glimpse" \
        --replace-quiet /bin/kill "${coreutils}/bin/kill" \
        --replace-quiet /usr/bin/test "${coreutils}/bin/test"
    done
    for target in data/systemd/*.target; do
      install -Dm644 "$target" "$out/lib/systemd/user/$(basename "$target")"
    done
    for service in data/dbus-1/services/*.service; do
      target="$out/share/dbus-1/services/$(basename "$service")"
      install -Dm644 "$service" "$target"
      substituteInPlace "$target" --replace-fail /usr/bin/glimpse "$out/bin/glimpse"
    done
    for portal in data/portals/*.portal; do
      install -Dm644 "$portal" "$out/share/xdg-desktop-portal/portals/$(basename "$portal")"
    done
    mkdir -p "$out/share/applications"
    msgfmt --desktop --template data/me.aresa.Glimpse.Shot.desktop -d po -o "$out/share/applications/me.aresa.Glimpse.Shot.desktop"
    msgfmt --desktop --template data/me.aresa.Glimpse.Lens.desktop -d po -o "$out/share/applications/me.aresa.Glimpse.Lens.desktop"
    msgfmt --desktop --template data/me.aresa.Glimpse.Lens.Ruler.desktop -d po -o "$out/share/applications/me.aresa.Glimpse.Lens.Ruler.desktop"
    msgfmt --desktop --template data/me.aresa.Glimpse.Lens.Picker.desktop -d po -o "$out/share/applications/me.aresa.Glimpse.Lens.Picker.desktop"
    msgfmt --desktop --template data/me.aresa.Glimpse.Shot.Annotate.desktop -d po -o "$out/share/applications/me.aresa.Glimpse.Shot.Annotate.desktop"
    substituteInPlace "$out/share/applications/me.aresa.Glimpse.Shot.desktop" \
      --replace-fail "Exec=gapplication" "Exec=${glib.bin}/bin/gapplication"
    install -Dm644 data/me.aresa.Glimpse.Shot.metainfo.xml "$out/share/metainfo/me.aresa.Glimpse.Shot.metainfo.xml"
    cp -r data/icons "$out/share/"

    substituteInPlace "$out/share/glimpse/config.default.toml" \
      --replace-fail /usr/share/glimpse "$out/share/glimpse"
    substituteInPlace "$out/share/glimpse/config.commented.toml" \
      --replace-fail /usr/share/glimpse "$out/share/glimpse"
    substituteInPlace "$out/share/glimpse/config.schema.json" \
      --replace-fail /usr/share/glimpse "$out/share/glimpse"

    while read -r language; do
      if [ -n "$language" ]; then
        mkdir -p "$out/share/locale/$language/LC_MESSAGES"
        msgfmt "po/$language.po" -o "$out/share/locale/$language/LC_MESSAGES/glimpse.mo"
      fi
    done < po/LINGUAS
  '';

  meta = {
    description = "Desktop shell suite for Wayland compositors";
    license = lib.licenses.bsd3;
    platforms = [ "x86_64-linux" ];
  };
}
