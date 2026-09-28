#!/usr/bin/env bash
# Fetches the real-world DOCX and DOC corpora the benchmarks run on, as
# sparse, shallow git checkouts. The files are other projects' test data
# under their own licenses: they are downloaded OUTSIDE the repo and never
# committed.
#
#   benchmarks/fetch_corpus.sh ~/corpora
#
# gives ~/corpora/{libreoffice,poi,python-docx,docx4j}. Revisions are pinned
# so a rerun measures the same files. Then, for example:
#
#   python benchmarks/bench.py docx ~/corpora/libreoffice ~/corpora/poi ~/corpora/python-docx
#   python benchmarks/bench.py doc  ~/corpora/libreoffice ~/corpora/poi
set -euo pipefail

dest="${1:?usage: fetch_corpus.sh DEST_DIR}"
mkdir -p "$dest"

fetch() {
    local name="$1" url="$2" rev="$3"
    shift 3
    local dir="$dest/$name"
    if [ ! -d "$dir/.git" ]; then
        git clone --quiet --filter=blob:none --no-checkout --depth 1 "$url" "$dir" || \
            git clone --quiet --filter=blob:none --no-checkout "$url" "$dir"
    fi
    git -C "$dir" sparse-checkout set --no-cone "$@"
    git -C "$dir" fetch --quiet --depth 1 origin "$rev" || true
    git -C "$dir" checkout --quiet "$rev" 2>/dev/null || git -C "$dir" checkout --quiet FETCH_HEAD
    echo "$name: $(find "$dir" -name '*.docx' | wc -l | tr -d ' ') docx, $(find "$dir" -name '*.doc' | wc -l | tr -d ' ') doc"
}

# LibreOffice's Writer import/export regression documents (MPL-2.0).
fetch libreoffice https://github.com/LibreOffice/core.git 370e262391c09ef9f592fc5ddee81a3340d32681 \
    /sw/qa/extras/ooxmlexport/data/ /sw/qa/extras/ooxmlimport/data/ \
    /sw/qa/extras/ww8export/data/ /sw/qa/extras/ww8import/data/

# Apache POI's HWPF/XWPF test documents, many from bug reports (Apache-2.0).
fetch poi https://github.com/apache/poi.git 942d95d85b15d0dfdb3bc9ba1b4f273f277757c8 /test-data/document/

# python-docx's acceptance and unit test documents (MIT).
fetch python-docx https://github.com/python-openxml/python-docx.git e45454602b53e8e572b179ccf1c91093ec9f4ed7 \
    /features/steps/test_files/ /tests/test_files/

# docx4j's sample documents (Apache-2.0).
fetch docx4j https://github.com/plutext/docx4j.git 0e8e7633ef46012e0d3603339728b83ed21d5045 /docx4j-samples-docx4j/sample-docs/
