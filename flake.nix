{
  description = "Niri: A scrollable-tiling Wayland compositor.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
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
        craneLib = crane.lib.${system}.overrideToolchain toolchain;

        craneArgs = {
          pname = "niri";
          version = self.rev or "dirty";

          src = nix-filter.lib.filter {
            root = ./.;
            include = [
              ./src
              ./niri-config
              ./Cargo.toml
              ./Cargo.lock
              ./resources
            ];
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            autoPatchelfHook
            clang
          ];

          buildInputs = with pkgs; [
            wayland
            systemd # For libudev
            seatd # For libseat
            libxkbcommon
            libinput
            mesa # For libgbm
            fontconfig
            stdenv.cc.cc.lib
            pipewire
          ];

          runtimeDependencies = with pkgs; [
            wayland
            mesa
            libglvnd # For libEGL
          ];

          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
        };

        cargoArtifacts = craneLib.buildDepsOnly craneArgs;
        niri = craneLib.buildPackage (craneArgs // {inherit cargoArtifacts;});
      in {
        formatter = pkgs.alejandra;

        checks.niri = niri;
        packages.default = niri;

        devShells.default = pkgs.mkShell.override {stdenv = pkgs.clangStdenv;} {
          inherit (niri) nativeBuildInputs buildInputs LIBCLANG_PATH;
          packages = niri.runtimeDependencies;

          # Force linking to libEGL, which is always dlopen()ed, and to
          # libwayland-client, which is always dlopen()ed except by the
          # obscure winit backend.
          RUSTFLAGS = map (a: "-C link-arg=${a}") [
            "-Wl,--push-state,--no-as-needed"
            "-lEGL"
            "-lwayland-client"
            "-Wl,--pop-state"
          ];
        };
      }
    );
}
