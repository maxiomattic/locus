# Browser shell (crates/locus-web) as a static bundle for the gateway's
# --web-root: index.html, locus.js, locus_bg.wasm. Same output as
# scripts/web-build.sh, which this calls for the bundle step.
#
# crates/locus-web is its own Cargo workspace (upstream iced 0.14), so it has
# its own lockfile and vendored deps. Update `hash` whenever
# crates/locus-web/Cargo.lock changes.
{
  lib,
  stdenv,
  makeRustPlatform,
  rustToolchain, # must include the wasm32-unknown-unknown target
  wasm-bindgen-cli, # must match the wasm-bindgen crate in Cargo.lock
  binaryen,
}:

let
  rustPlatform = makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };
  root = ../..;
  fs = lib.fileset;
  web = root + "/crates/locus-web";
in
stdenv.mkDerivation {
  pname = "locus-web";
  version = "0.1.0";

  # locus-web plus the crates it path-depends on. The root Cargo.toml is
  # there for their `*.workspace = true` fields.
  src = fs.toSource {
    inherit root;
    fileset = fs.unions [
      (root + "/Cargo.toml")
      (web + "/Cargo.toml")
      (web + "/Cargo.lock")
      (web + "/index.html")
      (web + "/src")
      (root + "/crates/locus-core")
      (root + "/crates/locus-client")
      (root + "/crates/locus-effects")
      (root + "/scripts/web-build.sh")
    ];
  };

  cargoRoot = "crates/locus-web";
  cargoDeps = rustPlatform.fetchCargoVendor {
    name = "locus-web-cargo-deps";
    src = fs.toSource {
      root = web;
      fileset = fs.unions [
        (web + "/Cargo.toml")
        (web + "/Cargo.lock")
      ];
    };
    hash = "sha256-ZjDuqBg/0C/lWD4KXTjmwmHTT+0AKRq/R41rhOc415o=";
  };

  nativeBuildInputs = [
    rustPlatform.cargoSetupHook
    rustToolchain
    wasm-bindgen-cli
    binaryen
  ];

  buildPhase = ''
    runHook preBuild
    export CARGO_TARGET_DIR="$PWD/target"
    cargo build --offline --frozen \
      --manifest-path crates/locus-web/Cargo.toml \
      --target wasm32-unknown-unknown --release
    patchShebangs scripts/web-build.sh
    scripts/web-build.sh --bundle \
      "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/locus_web.wasm" \
      "$PWD/dist/web"
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    cp -r dist/web "$out"
    runHook postInstall
  '';

  # Nothing to fix up in a static bundle.
  dontFixup = true;

  meta = {
    description = "Locus browser app (iced on wasm32), served by locus-gateway under /app/";
    platforms = lib.platforms.all;
  };
}
