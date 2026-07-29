# helper-kill Design

Add a phony `helper-kill` Make target that runs `pgrep kivo`, then sends the
default `SIGTERM` to every returned PID. If `pgrep` finds nothing, the target
exits successfully without invoking `kill`.

Keep the behavior in the root `Makefile`; no script or dependency is needed.
Verify both paths by invoking the target with stub `pgrep` and `kill` commands:
all returned PIDs reach one `kill` call, and an empty result remains successful.
