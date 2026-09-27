{
  fetchFromGitHub,
  rustPlatform,
  lib,
}:
rustPlatform.buildRustPackage (finalAttrs: {

  pname = "flow";
  # bump on new version release
  version = "0.0.1";

  src = fetchFromGitHub {
    owner = "jsubroto";
    repo = "flow";
    # bump on new version release
    rev = "73734e1404d87681ad951fe7dab0a7ebbe3105da";
    sha256 = "sha256-d7L0zBPMqXKWC0bJFz5c4VSkyJgF+EhljILqYSfgilQ=";
  };

  # bump on new version release
  cargoHash = "sha256-HA4puD7y5SJRbmjTrlHSehgApVr99jS3EopzeJ0kco4=";

  meta = {
    description = "A keyboard-first Kanban board for your terminal workflows";
    homepage = "https://github.com/jsubroto/flow";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ jsubroto ];
  };

})
