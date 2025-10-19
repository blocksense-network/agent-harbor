{ inputs, ... }:
{
  imports = [
    inputs.git-hooks.flakeModule
  ];

  perSystem =
    { ... }:
    {
      pre-commit = {
        settings = {
          hooks = {
          };
        };
      };
    };
}
