.PHONY: web agent-loop

web:
	cargo run

agent-loop:
	./scripts/agent-loop.sh