# This flake file is community maintained
{
  description = "Niri: A scrollable-tiling Wayland compositor.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    nix-filter.url = "github:numtide/nix-filter";
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    nix-filter,
    flake-utils,
    fenix,
    ...
  }: let
    systems = ["aarch64-linux" "x86_64-linux"];
  in
    flake-utils.lib.eachSystem systems (
      system: let
        pkgs = nixpkgs.legacyPackages.${system};
        toolchain = fenix.packages.${system}.complete.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        craneArgs = {
          pname = "niri";
          version = self.rev or "dirty";

          src = nixpkgs.lib.cleanSourceWith {
            src = craneLib.path ./.;
            filter = path: type:
              (builtins.match "resources" path == null)
              || ((craneLib.filterCargoSources path type)
                && (builtins.match "niri-visual-tests" path == null));
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            autoPatchelfHook
            clang
            gdk-pixbuf
            graphene
            gtk4
            libadwaita
          ];

          buildInputs = with pkgs; [
            wayland
            systemd # For libudev
            seatd # For libseat
            libxkbcommon
            libdisplay-info
            libinput
            mesa # For libgbm
            fontconfig
            stdenv.cc.cc.lib
            pipewire
            pango
            cairo
            glib
            pixman
          ];

          runtimeDependencies = with pkgs; [
            wayland
            mesa
            libglvnd # For libEGL
            xorg.libXcursor
            xorg.libXi
            libxkbcommon
          ];

          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath craneArgs.runtimeDependencies; # Needed for tests to find libxkbcommon
        };

        cargoArtifacts = craneLib.buildDepsOnly craneArgs;
        niri = craneLib.buildPackage (craneArgs // {inherit cargoArtifacts;});
      in {
        formatter = pkgs.alejandra;

        checks.niri = niri;
        packages.default = niri;

        devShells.default = craneLib.devShell {
          inputsFrom = [niri];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (craneArgs.runtimeDependencies ++ craneArgs.nativeBuildInputs ++ craneArgs.buildInputs);
          inherit (niri) LIBCLANG_PATH;
        };
      }
    );
}
