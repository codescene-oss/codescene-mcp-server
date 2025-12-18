create-executable:
	python3.13 -m nuitka --onefile \
	--include-data-dir=./src/docs=src/docs \
	--include-data-files=./cs=cs \
	--output-filename=cs-mcp \
	src/cs_mcp_server.py
