FROM python:3.13
WORKDIR /app
COPY src/requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
RUN curl https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh | bash -s -- -y
COPY . .
ENV PATH="/root/.local/bin:${PATH}"
CMD [ "python", "src/cs_mcp_server.py" ]