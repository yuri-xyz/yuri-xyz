{
  description = "yuri-xyz GitHub profile: space scene renderer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/26.05";
    crane.url = "github:ipetkov/crane/v0.24.0";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];
      # The renderer is proprietary (see LICENSE), so nixpkgs must be told to
      # build this one unfree package; everything else stays free-only.
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f system (
            import nixpkgs {
              inherit system;
              config.allowUnfreePredicate = pkg: nixpkgs.lib.getName pkg == "profile-art";
            }
          )
        );

      mkCrate =
        pkgs:
        let
          craneLib = crane.mkLib pkgs;
          src = craneLib.cleanCargoSource ./.;
          commonArgs = {
            inherit src;
            strictDeps = true;
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          inherit craneLib;
          package = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              meta = {
                description = "Offline renderer for the animated space scenes on the GitHub profile README";
                license = pkgs.lib.licenses.unfree;
                mainProgram = "profile-art";
                platforms = systems;
              };
            }
          );
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );
          fmt = craneLib.cargoFmt { inherit src; };
        };
    in
    {
      packages = forAllSystems (
        system: pkgs:
        let
          crate = mkCrate pkgs;
        in
        {
          profile-art = crate.package;
          default = crate.package;

          # Renders every scene in both theme variants as looping animated WebP
          # into ./assets of the current directory.
          render = pkgs.writeShellApplication {
            name = "render";
            runtimeInputs = [
              crate.package
              pkgs.libwebp
            ];
            text = builtins.readFile ./scripts/render.sh;
          };
        }
      );

      apps = forAllSystems (
        system: _: {
          render = {
            type = "app";
            program = nixpkgs.lib.getExe self.packages.${system}.render;
          };
        }
      );

      devShells = forAllSystems (
        system: pkgs:
        let
          crate = mkCrate pkgs;
        in
        {
          default = crate.craneLib.devShell {
            inputsFrom = [ crate.package ];
            packages = [
              pkgs.rust-analyzer
              pkgs.libwebp
              pkgs.statix
              pkgs.nixfmt
            ];
          };
        }
      );

      checks = forAllSystems (
        system: pkgs:
        let
          crate = mkCrate pkgs;
        in
        {
          inherit (crate) clippy fmt package;
        }
      );

      formatter = forAllSystems (_: pkgs: pkgs.nixfmt);
    };
}
