ARG MANYLINUX_IMAGE=quay.io/pypa/manylinux_2_28_x86_64
FROM ${MANYLINUX_IMAGE}

ARG PYTHON_BIN=/opt/python/cp313-cp313/bin/python
ENV PYTHON_BIN=${PYTHON_BIN}

COPY src/requirements.txt /tmp/requirements.txt
COPY scripts/build-linux-exe-inner.sh /usr/local/bin/build-linux-exe-inner.sh

RUN "${PYTHON_BIN}" -m pip install --upgrade pip \
    && if [ -f /opt/_internal/static-libs-for-embedding-only.tar.xz ]; then cd /opt/_internal && tar xf static-libs-for-embedding-only.tar.xz; fi \
    && "${PYTHON_BIN}" -m pip install -r /tmp/requirements.txt Nuitka \
    && chmod +x /usr/local/bin/build-linux-exe-inner.sh

WORKDIR /work
ENTRYPOINT ["/usr/local/bin/build-linux-exe-inner.sh"]
