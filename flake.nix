# This flake file is community maintained
{
  description = "Niri: A scrollable-tiling Wayland compositor.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nix-filter.url = "github:numtide/nix-filter";
  };

  outputs =
    {
      self,
      nixpkgs,
      nix-filter,
      ...
    }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "aarch64-linux"
        "x86_64-linux"
      ];

      forAllSystems = lib.genAttrs systems;
      nixpkgsFor = forAllSystems (system: nixpkgs.legacyPackages.${system});
    in
    {
      checks = forAllSystems (system: {
        inherit (self.packages.${system}) niri;
      });

      devShells = forAllSystems (
        system:
        let
          inherit (self.packages.${system}) niri;
        in
        {
          default = nixpkgsFor.${system}.mkShell {
            inputsFrom = [ niri ];

            env = {
              LD_LIBRARY_PATH = lib.makeLibraryPath niri.buildInputs;
              inherit (niri.env) LIBCLANG_PATH;
            };
          };
        }
      );

      formatter = forAllSystems (system: nixpkgsFor.${system}.nixfmt-rfc-style);

      packages = forAllSystems (system: {
        niri = nixpkgsFor.${system}.callPackage (
          {
            cairo,
            clang,
            fontconfig,
            gdk-pixbuf,
            glib,
            graphene,
            gtk4,
            libadwaita,
            libclang,
            libdisplay-info,
            libglvnd,
            libinput,
            libxkbcommon,
            mesa,
            pango,
            pipewire,
            pixman,
            pkg-config,
            rustPlatform,
            seatd,
            stdenv,
            systemd,
            wayland,
            xorg,
          }:
          rustPlatform.buildRustPackage rec {
            pname = "niri";
            version = self.shortRev or self.dirtyShortRev or "unknown";

            cargoLock = {
              # NOTE: This is only used for Git dependencies
              allowBuiltinFetchGit = true;
              lockFile = ./Cargo.lock;
            };

            src = nix-filter.lib.filter {
              root = self;
              include = [
                "niri-config"
                "niri-ipc"
                "resources"
                "src"
              ];
            };

            nativeBuildInputs = [
              clang
              gdk-pixbuf
              graphene
              gtk4
              libadwaita
              pkg-config
            ];

            buildInputs = [
              cairo
              fontconfig
              glib
              libdisplay-info
              libinput
              libxkbcommon
              mesa # For libgbm
              pango
              pipewire
              pixman
              seatd # For libseat
              stdenv.cc.cc.lib
              systemd # For libudev
              wayland
            ];

            runtimeDependencies = [
              libglvnd # For libEGL
              libxkbcommon
              mesa
              wayland
              xorg.libXcursor
              xorg.libXi
            ];

            LD_LIBRARY_PATH = lib.makeLibraryPath runtimeDependencies;
            LIBCLANG_PATH = lib.getLib libclang + "/lib";
          }
        ) { };

        default = self.packages.${system}.niri;
      });
    };
}
