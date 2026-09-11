.PHONY: web agent-loop restart

web:
	cargo run -- --port 8065

agent-loop:
	./scripts/agent-loop.sh

# Build the release binary and restart the deployed user service. Run on
# the server as `web`. The post-receive hook runs the same script on every
# push; this target is the manual equivalent.
restart:
	./deploy/scripts/build-and-restart
