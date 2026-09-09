.PHONY: web agent-loop

web:
	cargo run -- --port 8065

agent-loop:
	./scripts/agent-loop.sh
