#!/usr/bin/env bash
# Download / generate real-format fixtures for `tests/real_formats.rs`.
#
# EPUBs come from Project Gutenberg (stable, public domain).
# DOCX and XLSX are generated inline via Python so no external host is
# required -- the files are minimal but valid Office Open XML documents
# with enough text content to satisfy the integration test assertions.
#
# Re-running is idempotent: files are only written if absent.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/target/test-fixtures/real"
mkdir -p "$DEST"

fetch_if_missing() {
    local url=$1 out=$2
    if [[ -f "$out" && -s "$out" ]]; then
        printf '==> %s already present\n' "${out#"$ROOT"/}"
        return
    fi
    printf '==> downloading %s\n' "$url"
    curl --fail --location --silent --show-error --output "$out" "$url"
}

# -----------------------------------------------------------------------------
# EPUBs: Project Gutenberg
# -----------------------------------------------------------------------------
fetch_if_missing \
    'https://www.gutenberg.org/ebooks/11.epub3.images' \
    "$DEST/alice.epub"

fetch_if_missing \
    'https://www.gutenberg.org/ebooks/5200.epub3.images' \
    "$DEST/metamorphosis.epub"

# -----------------------------------------------------------------------------
# DOCX + XLSX: generated via Python (stdlib only)
# -----------------------------------------------------------------------------
command -v python3 >/dev/null || { echo 'python3 required for DOCX/XLSX generation'; exit 1; }

python3 - "$DEST" <<'PY'
import sys, os, zipfile

dest = sys.argv[1]

def write_zip(path, files):
    if os.path.exists(path) and os.path.getsize(path) > 0:
        print(f'==> {path} already present')
        return
    print(f'==> generating {path}')
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as zf:
        for name, data in files.items():
            zf.writestr(name, data)

# ---- DOCX --------------------------------------------------------------------
docx_text = (
    'This is a generated test fixture for the deformat DOCX extractor. '
    'It contains enough lorem ipsum text to satisfy the integration test '
    'assertions that require at least one hundred characters of extracted '
    'prose from a real DOCX file. Additional filler content follows so '
    'that simple length-based checks succeed on this minimal workbook.'
)
docx = {
    '[Content_Types].xml': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>''',
    '_rels/.rels': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>''',
    'word/document.xml': f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t xml:space="preserve">{docx_text}</w:t></w:r></w:p>
</w:body>
</w:document>''',
}
write_zip(os.path.join(dest, 'sample1.docx'), docx)

# ---- XLSX --------------------------------------------------------------------
strings = ['Name', 'Score', 'Alice', 'Bob']
shared_strings = ''.join(f'<si><t>{s}</t></si>' for s in strings)
xlsx = {
    '[Content_Types].xml': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>''',
    '_rels/.rels': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>''',
    'xl/workbook.xml': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>
</workbook>''',
    'xl/_rels/workbook.xml.rels': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>''',
    'xl/sharedStrings.xml': f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{len(strings)}" uniqueCount="{len(strings)}">{shared_strings}</sst>''',
    'xl/worksheets/sheet1.xml': '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
<row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>42</v></c></row>
<row r="3"><c r="A3" t="s"><v>3</v></c><c r="B3"><v>37</v></c></row>
</sheetData>
</worksheet>''',
}
write_zip(os.path.join(dest, 'sample1.xlsx'), xlsx)
PY

echo
echo "Fixtures ready in: $DEST"
ls -la "$DEST"
