
.PHONY: copy-install-script
copy-install-script:
	gsutil rm gs://nebulous/releases/install.sh
	gsutil cp install.sh gs://nebulous/releases/install.sh
	gsutil setmeta \
		-h "Cache-Control: no-store, no-cache, must-revalidate, proxy-revalidate, max-age=0" \
		-h "Content-Type: application/x-sh" \
		gs://nebulous/releases/install.sh

.PHONY: install
install:
	cargo build
	sudo cp target/debug/nebulous ~/.local/bin/nebulous


.PHONY: test-prepare
test-prepare:
	cargo build
	./target/debug/nebulous prepare -d swift -u https://storage.googleapis.com/agentsea-dev-hub-exports/exports/2024-12-02/3923647e-1cdc-4ebc-8dde-e00ff3a0c99c -s 0.8 -b ./scratch
