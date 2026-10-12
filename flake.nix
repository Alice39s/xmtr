{
  description = "Concurrent traceroute that maps every hop on xmsl.dev IPMap";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      workspace = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package;
    in
    {
      packages = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.xmtr;
        xmtr = pkgs.rustPlatform.buildRustPackage {
          pname = "xmtr";
          inherit (workspace) version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "--package"
            "xmtr"
          ];
          cargoTestFlags = [
            "--package"
            "xmtr"
          ];
          meta = {
            description = "Concurrent traceroute that maps every hop on xmsl.dev IPMap";
            homepage = "https://github.com/Alice39s/xmtr";
            license = with pkgs.lib.licenses; [
              mit
              asl20
            ];
            mainProgram = "xmtr";
            platforms = systems;
          };
        };
      });
    };
}
