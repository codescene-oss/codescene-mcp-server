FROM python:3.13
WORKDIR /app
COPY src/requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
RUN curl https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh | bash -s -- -y
COPY . .
COPY src/ /app/src

# Inject version during build (passed as build arg)
ARG VERSION=dev
RUN sed -i "s/__version__ = \"dev\"/__version__ = \"${VERSION}\"/" /app/src/version.py

ENV PATH="/root/.local/bin:${PATH}"
LABEL io.modelcontextprotocol.server.name="com.codescene/codescene-mcp-server"
CMD [ "python", "src/cs_mcp_server.py" ]