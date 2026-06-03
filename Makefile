.PHONY: build
build:
	cargo build --release

.PHONY: test
test:
	cargo test

.PHONY: test-e2e
test-e2e:
	@echo "Running e2e tests..."
	cargo test --test e2e

.PHONY: test-all
test-all:
	@echo "Running unit tests..."
	cargo test
	@echo ""
	@echo "Running npm unit tests..."
	cd npm && npm test
	@echo ""
	@echo "Running e2e tests..."
	cargo test --test e2e

.PHONY: lint
lint:
	cargo clippy -- -D warnings

.PHONY: lint-fix
lint-fix:
	cargo clippy --fix --allow-dirty

.PHONY: format
format:
	cargo fmt

.PHONY: format-check
format-check:
	cargo fmt -- --check
