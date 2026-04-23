#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Generate the synthetic fixture files under tests/fixtures/synthetic/.

Deterministic: running this script twice produces byte-identical output.
All fixtures authored here are dual-licensed MIT-OR-Apache-2.0 along
with the rest of the crate. See tests/fixtures/PROVENANCE.md.

Usage:
    uv run scripts/generate_synthetic_fixtures.py
    # or: python3 scripts/generate_synthetic_fixtures.py

Re-run after editing this file; commit both the script and the
generated fixtures so CI has a fixed corpus without a fetch step.
"""

from __future__ import annotations

import zipfile
from io import BytesIO
from pathlib import Path


def write_zip(path: Path, files: dict[str, str | bytes]) -> None:
    buf = BytesIO()
    # Force deterministic metadata so regeneration yields the same bytes.
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for name in sorted(files):
            data = files[name]
            if isinstance(data, str):
                data = data.encode("utf-8")
            info = zipfile.ZipInfo(name)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.date_time = (2026, 1, 1, 0, 0, 0)
            zf.writestr(info, data)
    path.write_bytes(buf.getvalue())
    print(f"  wrote {path.name:30s} {len(buf.getvalue()):>6} bytes")


# ---------------------------------------------------------------------------
# DOCX fixtures
# ---------------------------------------------------------------------------

DOCX_CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""

DOCX_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""


def docx_document(body_inner: str) -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">\n'
        "<w:body>\n"
        f"{body_inner}\n"
        "</w:body>\n"
        "</w:document>"
    )


def write_docx(path: Path, body_inner: str) -> None:
    write_zip(
        path,
        {
            "[Content_Types].xml": DOCX_CONTENT_TYPES,
            "_rels/.rels": DOCX_RELS,
            "word/document.xml": docx_document(body_inner),
        },
    )


def docx_paragraph(text: str, style: str | None = None) -> str:
    style_xml = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
    return f'<w:p>{style_xml}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>'


# ---------------------------------------------------------------------------
# XLSX fixtures
# ---------------------------------------------------------------------------


def xlsx_content_types(sheet_count: int) -> str:
    sheet_overrides = "\n".join(
        f'<Override PartName="/xl/worksheets/sheet{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        for i in range(1, sheet_count + 1)
    )
    return f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
{sheet_overrides}
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>"""


XLSX_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"""


def xlsx_workbook(sheets: list[str]) -> str:
    sheet_xml = "\n".join(
        f'<sheet name="{name}" sheetId="{i + 1}" r:id="rId{i + 1}"/>'
        for i, name in enumerate(sheets)
    )
    return f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>{sheet_xml}</sheets>
</workbook>"""


def xlsx_workbook_rels(sheet_count: int) -> str:
    sheet_rels = "\n".join(
        f'<Relationship Id="rId{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i}.xml"/>'
        for i in range(1, sheet_count + 1)
    )
    return f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
{sheet_rels}
<Relationship Id="rId{sheet_count + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"""


def xlsx_shared_strings(strings: list[str]) -> str:
    items = "".join(f"<si><t>{s}</t></si>" for s in strings)
    return f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{len(strings)}" uniqueCount="{len(strings)}">{items}</sst>"""


def xlsx_worksheet(rows: list[list[tuple[str, str]]]) -> str:
    """rows: list of row, each row is list of (ref, cell_xml_inner)."""
    row_xml = []
    for i, row in enumerate(rows, 1):
        cells = "".join(f'<c r="{ref}" t="s"><v>{value}</v></c>' for ref, value in row)
        row_xml.append(f'<row r="{i}">{cells}</row>')
    return f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
{chr(10).join(row_xml)}
</sheetData>
</worksheet>"""


# ---------------------------------------------------------------------------
# PPTX fixture
# ---------------------------------------------------------------------------

PPTX_CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>"""

PPTX_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"""


# ---------------------------------------------------------------------------
# EPUB fixture
# ---------------------------------------------------------------------------

EPUB_MIMETYPE = "application/epub+zip"

EPUB_CONTAINER = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles>
<rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
</rootfiles>
</container>"""


def epub_opf(title: str, chapters: list[str]) -> str:
    manifest_items = "\n".join(
        f'<item id="ch{i}" href="chapter{i}.xhtml" media-type="application/xhtml+xml"/>'
        for i in range(1, len(chapters) + 1)
    )
    spine_items = "\n".join(
        f'<itemref idref="ch{i}"/>' for i in range(1, len(chapters) + 1)
    )
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="bookid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>{title}</dc:title>
<dc:language>en</dc:language>
<dc:identifier id="bookid">urn:uuid:deformat-fixture-{title.lower().replace(' ', '-')}</dc:identifier>
</metadata>
<manifest>
{manifest_items}
</manifest>
<spine>
{spine_items}
</spine>
</package>"""


def epub_chapter(title: str, body: str) -> str:
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title></head>
<body>
<h1>{title}</h1>
{body}
</body>
</html>"""


# ---------------------------------------------------------------------------
# RTF fixtures
# ---------------------------------------------------------------------------

MINIMAL_RTF = br"{\rtf1\ansi\ansicpg1252 Hello from RTF. Second sentence here.}"

# Windows-1252 codepage + \uN? Unicode fallback for non-ASCII.
# "Café in \u321?\u243?d\u378?" -> "Café in Łódź" (the ? is the ANSI fallback char).
UNICODE_RTF = (
    br"{\rtf1\ansi\ansicpg1252 "
    br"{\uc1 Caf\u233? in \u321?\u243?d\u378? and "
    br" hello world with punctuation.}}"
)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    out = root / "tests" / "fixtures" / "synthetic"
    out.mkdir(parents=True, exist_ok=True)

    print("== DOCX ==")
    # Minimal: single paragraph, ASCII prose.
    write_docx(
        out / "minimal.docx",
        docx_paragraph(
            "Hello from DOCX. This fixture is minimal but valid Office Open XML. "
            "It contains enough prose to exercise the extractor's text-length "
            "heuristics."
        ),
    )

    # Unicode: CJK + accented characters + punctuation.
    unicode_body = "\n".join(
        [
            docx_paragraph(
                "Heading: Unicode Round-Trip Test", style="Heading1"
            ),
            docx_paragraph(
                "Café in München met Łódź."
            ),
            docx_paragraph("中文测试 内容字符串。"),
            docx_paragraph(
                "Japanese: こんにちは、世界！"
            ),
        ]
    )
    write_docx(out / "unicode.docx", unicode_body)

    # Table: <w:tbl> with two rows, three columns.
    table_body = (
        docx_paragraph("Table fixture")
        + "\n"
        + """<w:tbl>
<w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr>
<w:tr>
<w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>Note</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
<w:tc><w:p><w:r><w:t>Alpha</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>42</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>First row body.</w:t></w:r></w:p></w:tc>
</w:tr>
</w:tbl>"""
    )
    write_docx(out / "table.docx", table_body)

    print("\n== XLSX ==")
    # Minimal: single sheet, four cells.
    strings = ["Name", "Score", "Alice", "Bob"]
    write_zip(
        out / "minimal.xlsx",
        {
            "[Content_Types].xml": xlsx_content_types(1),
            "_rels/.rels": XLSX_RELS,
            "xl/workbook.xml": xlsx_workbook(["Sheet1"]),
            "xl/_rels/workbook.xml.rels": xlsx_workbook_rels(1),
            "xl/sharedStrings.xml": xlsx_shared_strings(strings),
            "xl/worksheets/sheet1.xml": xlsx_worksheet(
                [[("A1", "0"), ("B1", "1")], [("A2", "2"), ("B2", "3")]]
            ),
        },
    )

    # Unicode + multi-sheet.
    strings_u = [
        "Name",
        "Score",
        "中文",  # CJK
        "Café",
        "Привет",  # Cyrillic
        "Note",
        "Summary",
        "Totals at bottom.",
    ]
    write_zip(
        out / "unicode.xlsx",
        {
            "[Content_Types].xml": xlsx_content_types(2),
            "_rels/.rels": XLSX_RELS,
            "xl/workbook.xml": xlsx_workbook(["Data", "Notes"]),
            "xl/_rels/workbook.xml.rels": xlsx_workbook_rels(2),
            "xl/sharedStrings.xml": xlsx_shared_strings(strings_u),
            "xl/worksheets/sheet1.xml": xlsx_worksheet(
                [
                    [("A1", "0"), ("B1", "1")],
                    [("A2", "2"), ("B2", "3")],
                    [("A3", "4"), ("B3", "3")],
                ]
            ),
            "xl/worksheets/sheet2.xml": xlsx_worksheet(
                [[("A1", "5")], [("A2", "6")], [("A3", "7")]]
            ),
        },
    )

    print("\n== PPTX ==")
    # Minimal presentation: one slide with a title + body text run.
    slide_xml = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree>
<p:sp><p:txBody>
<a:p><a:r><a:t>Slide Title Text</a:t></a:r></a:p>
<a:p><a:r><a:t>Slide body paragraph with enough prose to pass length checks.</a:t></a:r></a:p>
</p:txBody></p:sp>
</p:spTree></p:cSld>
</p:sld>"""
    pres_xml = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>
</p:presentation>"""
    pres_rels = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>"""
    write_zip(
        out / "minimal.pptx",
        {
            "[Content_Types].xml": PPTX_CONTENT_TYPES,
            "_rels/.rels": PPTX_RELS,
            "ppt/presentation.xml": pres_xml,
            "ppt/_rels/presentation.xml.rels": pres_rels,
            "ppt/slides/slide1.xml": slide_xml,
        },
    )

    print("\n== EPUB ==")
    # Minimal: two chapters.
    chapters = [
        epub_chapter(
            "Chapter One",
            "<p>First chapter body. Introduces the fixture narrative.</p>"
            "<p>A second paragraph to exercise multi-paragraph extraction.</p>",
        ),
        epub_chapter(
            "Chapter Two",
            "<p>Second chapter continues the story.</p>"
            "<p>Café au lait and 中文 for Unicode coverage.</p>",
        ),
    ]
    epub_files = {
        "mimetype": EPUB_MIMETYPE,
        "META-INF/container.xml": EPUB_CONTAINER,
        "OEBPS/content.opf": epub_opf("Deformat Fixture", chapters),
    }
    for i, ch in enumerate(chapters, 1):
        epub_files[f"OEBPS/chapter{i}.xhtml"] = ch
    # EPUB spec requires `mimetype` as the first (uncompressed) entry.
    # Write it in a way that results in reproducible bytes.
    buf = BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        mt_info = zipfile.ZipInfo("mimetype")
        mt_info.compress_type = zipfile.ZIP_STORED
        mt_info.date_time = (2026, 1, 1, 0, 0, 0)
        zf.writestr(mt_info, EPUB_MIMETYPE)
        for name in sorted(epub_files):
            if name == "mimetype":
                continue
            info = zipfile.ZipInfo(name)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.date_time = (2026, 1, 1, 0, 0, 0)
            zf.writestr(info, epub_files[name])
    (out / "minimal.epub").write_bytes(buf.getvalue())
    print(f"  wrote minimal.epub                 {len(buf.getvalue()):>6} bytes")

    print("\n== RTF ==")
    (out / "minimal.rtf").write_bytes(MINIMAL_RTF)
    print(f"  wrote minimal.rtf                  {len(MINIMAL_RTF):>6} bytes")
    (out / "unicode.rtf").write_bytes(UNICODE_RTF)
    print(f"  wrote unicode.rtf                  {len(UNICODE_RTF):>6} bytes")

    print("\n== done ==")
    total = sum(p.stat().st_size for p in out.glob("*"))
    print(f"total fixture size: {total} bytes in {out}")


if __name__ == "__main__":
    main()
