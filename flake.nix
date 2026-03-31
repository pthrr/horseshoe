{
  description = "Wayland terminal emulator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      # Overlay: adds `pkgs.horseshoe` to any nixpkgs that applies it.
      # Usage in Home Manager:
      #   nixpkgs.overlays = [ horseshoe.overlays.default ];
      #   home.packages = [ pkgs.horseshoe ];
      overlay = final: prev:
        let
          rustVersion = "1.93.0";
          rustToolchain = final.rust-bin.stable.${rustVersion}.default.override {
            targets = [ "x86_64-unknown-linux-musl" "aarch64-unknown-linux-musl" ];
            extensions = [ "rust-src" "rust-analyzer" "clippy" "llvm-tools" ];
          };
          buildDeps = with final; [ wayland wayland-protocols libxkbcommon ];

          # Extract ghostty commit from build.rs (single source of truth).
          ghosttyCommit = let
            content = builtins.readFile ./crates/libghostty-vt-sys/build.rs;
            m = builtins.match ''.*GHOSTTY_COMMIT: &str = "([0-9a-f]+)".*'' content;
          in
            if m == null then builtins.throw "Cannot extract GHOSTTY_COMMIT from build.rs"
            else builtins.head m;

          ghosttySrc = final.fetchgit {
            url = "https://github.com/ghostty-org/ghostty.git";
            rev = ghosttyCommit;
            hash = "sha256-7MPEjIAQD+Z/zdP4h/yslysuVnhCESOPvdvwoLoPVmI=";
            fetchSubmodules = false;
          };

          libghosttyVt = final.stdenv.mkDerivation {
            pname = "libghostty-vt";
            version = "0-unstable-${builtins.substring 0 7 ghosttyCommit}";
            src = ghosttySrc;
            nativeBuildInputs = [ final.zig_0_15 ];

            dontUseZigBuild = true;
            dontUseZigCheck = true;
            dontUseZigInstall = true;
            dontConfigure = true;
            dontFixup = true;

            buildPhase =
              let zigDeps = final.callPackage "${ghosttySrc}/build.zig.zon.nix" {
                name = "ghostty-zig-deps";
              };
              in ''
                mkdir -p $TMPDIR/zig-cache
                ln -s ${zigDeps} $TMPDIR/zig-cache/p
                export ZIG_GLOBAL_CACHE_DIR=$TMPDIR/zig-cache
                export ZIG_LOCAL_CACHE_DIR=$TMPDIR/zig-local
                zig build -Demit-lib-vt -Dsimd=false --prefix $out
              '';
            dontInstall = true;
          };
        in {
          horseshoe = (final.makeRustPlatform {
            rustc = rustToolchain;
            cargo = rustToolchain;
          }).buildRustPackage {
            pname = "horseshoe";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = with final; [ pkg-config wayland-scanner ];
            buildInputs = buildDeps;

            # Point build.rs at the pre-built library; skips zig entirely.
            LIBGHOSTTY_VT_DIR = "${libghosttyVt}";

            # Integration tests need a real PTY + bash which the sandbox lacks.
            doCheck = false;

            meta = with final.lib; {
              description = "Wayland terminal emulator";
              license = licenses.mit;
              platforms = platforms.linux;
              mainProgram = "hs";
            };
          };
        };
    in
    {
      overlays.default = overlay;
    } //
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default overlay ];
        };

        # Pinned Rust toolchain with musl targets for static release builds.
        # Bump this version deliberately — don't let it float with nixpkgs.
        rustVersion = "1.93.0";
        rustToolchain = pkgs.rust-bin.stable.${rustVersion}.default.override {
          targets = [ "x86_64-unknown-linux-musl" "aarch64-unknown-linux-musl" ];
          extensions = [ "rust-src" "rust-analyzer" "clippy" "llvm-tools" ];
        };

        # Musl cross-compilation toolchains
        x86MuslPkgs = pkgs.pkgsCross.musl64;
        x86MuslCC = x86MuslPkgs.stdenv.cc;
        x86MuslXkbcommon = x86MuslPkgs.pkgsStatic.libxkbcommon;
        x86MuslWayland = x86MuslPkgs.pkgsStatic.wayland;

        arm64MuslPkgs = pkgs.pkgsCross.aarch64-multiplatform-musl;
        arm64MuslCC = arm64MuslPkgs.stdenv.cc;
        arm64MuslXkbcommon = arm64MuslPkgs.pkgsStatic.libxkbcommon;
        arm64MuslWayland = arm64MuslPkgs.pkgsStatic.wayland;

        # Build-time tools that run on the build host:
        # - zig_0_15: libghostty-vt-sys compiles libghostty-vt.a via Zig
        # - pkg-config: locates wayland and xkbcommon libraries
        # - wayland-scanner: generates Wayland protocol marshalling code
        nativeBuildDeps = with pkgs; [
          zig_0_15
          pkg-config
          wayland-scanner
        ];

        # Libraries linked into the final binary:
        # - wayland: Wayland client protocol (wl_display, wl_surface, etc.)
        # - wayland-protocols: xdg-shell, xdg-activation, xdg-decoration
        # - libxkbcommon: XKB keymap compilation and keysym resolution
        buildDeps = with pkgs; [
          wayland
          wayland-protocols
          libxkbcommon
        ];
      in
      {
        # Nix package — reuse the overlay definition
        packages.default = pkgs.horseshoe;

        # Development shell — provides Rust toolchain, Zig, and all dependencies
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = nativeBuildDeps ++ [
            rustToolchain
            pkgs.cargo-llvm-cov
            pkgs.go-task
            pkgs.uv
            pkgs.wl-clipboard
            pkgs.wtype
          ];

          buildInputs = buildDeps;

          # LD_LIBRARY_PATH for dynamic linking during debug builds
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildDeps;

          # Static musl build environment — these variables let
          # `cargo build --target x86_64-unknown-linux-musl --release`
          # find the static libxkbcommon.a for linking.
          PKG_CONFIG_ALLOW_CROSS = "1";

          # Target-specific static lib dirs (read by build.rs)
          XKBCOMMON_LIB_DIR_X86_64_UNKNOWN_LINUX_MUSL = "${x86MuslXkbcommon}/lib";
          XKBCOMMON_LIB_DIR_AARCH64_UNKNOWN_LINUX_MUSL = "${arm64MuslXkbcommon}/lib";

          # Target-specific pkg-config paths for static musl builds
          # Cross-linkers on PATH (via shellHook, NOT nativeBuildInputs,
          # to avoid Nix setup hooks overwriting CC for host builds).
          shellHook = ''
            export PATH="${x86MuslCC}/bin:${arm64MuslCC}/bin:$PATH"
            export PKG_CONFIG_PATH_x86_64_unknown_linux_musl="${x86MuslXkbcommon.dev}/lib/pkgconfig:${x86MuslWayland.dev}/lib/pkgconfig"
            export PKG_CONFIG_SYSROOT_DIR_x86_64_unknown_linux_musl="/"
            export PKG_CONFIG_PATH_aarch64_unknown_linux_musl="${arm64MuslXkbcommon.dev}/lib/pkgconfig:${arm64MuslWayland.dev}/lib/pkgconfig"
            export PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_musl="/"
          '';
        };
      });
}
