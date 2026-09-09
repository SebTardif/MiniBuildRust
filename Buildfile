# Example Buildfile for minibuild
env GREETING = Hello from Minibuild

default all

rule all
  deps greet compute
  description Build everything
  phony true
  run echo "All done!"

rule greet
  deps setup
  inputs /tmp/minibuild_demo/setup.stamp
  outputs /tmp/minibuild_demo/greet.out
  run echo "$GREETING" | tee /tmp/minibuild_demo/greet.out

rule compute
  deps setup
  inputs /tmp/minibuild_demo/setup.stamp
  outputs /tmp/minibuild_demo/compute.out
  run echo "Computing result: 42" | tee /tmp/minibuild_demo/compute.out

rule setup
  inputs Buildfile
  outputs /tmp/minibuild_demo/setup.stamp
  run mkdir -p /tmp/minibuild_demo && echo "Setup complete" | tee /tmp/minibuild_demo/setup.stamp
