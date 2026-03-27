.PHONY: build
build:
	cargo build --release

.PHONY: test
test:
	cargo test

.PHONY: test-integration
test-integration:
	@echo "Running comprehensive integration tests..."
	./tests/run-integration-tests.sh

.PHONY: test-integration-platform
test-integration-platform:
	@echo "Running platform-specific integration tests..."
	@if [ ! -f "../cs_mcp_test_bin/cs-mcp" ]; then \
		echo "No executable found. Building first..."; \
		./tests/run-integration-tests.sh; \
	else \
		./tests/run-integration-tests.sh --platform-only --executable ../cs_mcp_test_bin/cs-mcp; \
	fi

.PHONY: test-integration-worktree
test-integration-worktree:
	@echo "Running git worktree integration tests..."
	@if [ ! -f "../cs_mcp_test_bin/cs-mcp" ]; then \
		echo "No executable found. Building first..."; \
		./tests/run-integration-tests.sh; \
	else \
		./tests/run-integration-tests.sh --worktree-only --executable ../cs_mcp_test_bin/cs-mcp; \
	fi

.PHONY: test-npm-package
test-npm-package:
	@echo "Running npm wrapper integration tests..."
	@if [ ! -f "../cs_mcp_test_bin/cs-mcp" ]; then \
		echo "No executable found. Building first..."; \
		./tests/run-integration-tests.sh --npm; \
	else \
		./tests/run-integration-tests.sh --npm --executable ../cs_mcp_test_bin/cs-mcp; \
	fi

.PHONY: test-all
test-all:
	@echo "Running unit tests..."
	cargo test
	@echo ""
	@echo "Running npm unit tests..."
	cd npm && npm test
	@echo ""
	@echo "Running integration tests..."
	./tests/run-integration-tests.sh

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
