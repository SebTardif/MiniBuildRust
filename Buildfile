# Example Buildfile for minibuild
env GREETING = Hello from Minibuild

default all

rule all
  deps greet compute
  description Build everything
  phony true
  run echo "All done!"

rule greet
  run echo "$GREETING"

rule compute
  deps setup
  run echo "Computing result: 42"

rule setup
  run mkdir -p /tmp/minibuild_demo && echo "Setup complete"