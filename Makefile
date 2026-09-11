.PHONY: web agent-loop deploy

web:
	cargo run -- --port 8065

agent-loop:
	./scripts/agent-loop.sh

# Build the release binary and put it into service: restart the deployed
# user service. Run on the server as `web`, after the git hook has checked
# out a push.
deploy:
	cargo build --release
	XDG_RUNTIME_DIR=/run/user/$$(id -u) systemctl --user restart genbb
