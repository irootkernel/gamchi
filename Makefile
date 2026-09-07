.PHONY: build fmt fmt-check vet test-docs test-unit test-prepare test clean

BINARY_NAME := grokgrok
BIN_DIR := bin

build:
	go build -o $(BIN_DIR)/$(BINARY_NAME) ./cmd/grokgrok/

fmt:
	gofmt -w cmd internal scripts

fmt-check:
	@unformatted=$$(gofmt -l cmd internal scripts); \
	if [ -n "$$unformatted" ]; then \
		echo "unformatted files:"; echo "$$unformatted"; exit 1; \
	fi

vet:
	go vet ./...

test-docs:
	go run ./scripts/docscheck

test-unit:
	go test ./...

test-prepare:
	$(MAKE) fmt-check
	$(MAKE) test-docs
	$(MAKE) vet

test:
	$(MAKE) test-prepare
	$(MAKE) test-unit

clean:
	rm -rf $(BIN_DIR)
