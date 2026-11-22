"""Offline, paginated PDF and editable Word exports of source and recorded output."""

import base64
import io
import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast
from xml.sax.saxutils import escape

from .ledger import fold
from .notebook import Notebook, source_hash
from .sharing import payload
from .source import SourceError, atomic


@dataclass
class Block:
    kind: str
    text: str = ""
    level: int = 0
    data: bytes = b""
    rows: list[list[str]] | None = None


def clean(text: str) -> str:
    # XML 1.0 controls and terminal ANSI sequences are not document content.
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f]", "", re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text))


def markdown_blocks(text: str) -> list[Block]:
    from markdown_it import MarkdownIt

    tokens = MarkdownIt("commonmark").enable("table").parse(clean(text))
    blocks: list[Block] = []
    lists: list[int] = []
    prefix = ""
    heading = 0
    rows: list[list[str]] | None = None
    for token in tokens:
        if token.type in ("bullet_list_open", "ordered_list_open"):
            lists.append(
                int(token.attrGet("start") or 1) if token.type == "ordered_list_open" else 0
            )
        elif token.type in ("bullet_list_close", "ordered_list_close"):
            lists.pop()
        elif token.type == "list_item_open":
            prefix = "  " * (len(lists) - 1) + (f"{lists[-1]}. " if lists[-1] else "• ")
            if lists[-1]:
                lists[-1] += 1
        elif token.type == "heading_open":
            heading = int(token.tag[1])
        elif token.type == "heading_close":
            heading = 0
        elif token.type in ("fence", "code_block"):
            blocks.append(Block("code", token.content.rstrip("\n")))
        elif token.type == "table_open":
            rows = []
        elif token.type == "tr_open" and rows is not None:
            rows.append([])
        elif token.type == "table_close":
            blocks.append(Block("table", rows=rows))
            rows = None
        elif token.type == "inline":
            # Inline markup becomes editable text; images/links never cause network access.
            text = "".join(
                f"[Image: {child.content}]"
                if child.type == "image"
                else "\n"
                if child.type in ("softbreak", "hardbreak")
                else child.content
                if child.type in ("text", "code_inline", "html_inline")
                else ""
                for child in token.children or []
            )
            if rows is not None:
                rows[-1].append(text)
            else:
                blocks.append(Block("heading" if heading else "paragraph", prefix + text, heading))
            prefix = ""
        elif token.type == "html_block":
            blocks.append(Block("code", token.content))
        elif token.type == "hr":
            blocks.append(Block("paragraph", "────────"))
    return blocks


def notebook_blocks(notebook: Notebook, ledger: Any = None, blobs: Any = None) -> list[Block]:
    manifest = notebook.read()
    title = str(manifest["metadata"].get("title") or notebook.root.name)
    blocks = [Block("title", title)]
    records: dict[str, Any] = {}
    if ledger is not None:
        # Match the latest recorded execution per cell across sessions; never execute code.
        for row in ledger.rows(
            "SELECT e.* FROM executions e JOIN sessions s USING(session_id) "
            "ORDER BY s.start,s.session_id,e.ordinal"
        ):
            if row["cell_id"]:
                records[row["cell_id"]] = row
    for index, cell in enumerate(manifest["cells"], 1):
        source = notebook.source(cell)
        if cell["type"] == "markdown":
            blocks.extend(markdown_blocks(source))
            continue
        kind_label = (
            "Python"
            if cell["type"] == "code"
            else f"SQL → {cell.get('metadata', {}).get('output_var') or '_sql_df'}"
            if cell["type"] == "sql"
            else "Raw text"
        )
        blocks.append(Block("label", f"Cell {index} · {kind_label}"))
        blocks.append(Block("code", clean(source)))
        if cell["type"] not in ("code", "sql") or cell["id"] not in records:
            continue
        record = records[cell["id"]]
        blocks.append(
            Block("label", f"Recorded output · execution {record['ordinal']} · {record['status']}")
        )
        if record["source_sha256"] != source_hash(source):
            blocks.append(
                Block(
                    "warning", "These outputs were recorded from an earlier version of this cell."
                )
            )
        assert ledger is not None
        for event in fold(ledger.events(record["exec_id"])):
            kind = event["kind"]
            if kind in ("stdout", "stderr"):
                text = event.get("text")
                if text is None:
                    text = blobs.get(event["blob"]).decode("utf-8", errors="replace")
                blocks.append(Block("output", clean(text)))
            elif kind == "error":
                blocks.append(Block("warning", clean(f"{event['ename']}: {event['evalue']}")))
                trace = event.get("traceback", [])
                blocks.append(
                    Block(
                        "output",
                        clean(
                            "\n".join(cast(list[str], trace))
                            if isinstance(trace, list)
                            else str(trace)
                        ),
                    )
                )
            elif kind == "truncated":
                blocks.append(
                    Block(
                        "warning",
                        "Recorded output was truncated by the runtime; some output was not retained.",
                    )
                )
            elif kind in ("display", "result"):
                bundle = event.get("bundle", {})
                image_mime = next(
                    (mime for mime in ("image/png", "image/jpeg") if mime in bundle), None
                )
                if image_mime:
                    image = payload(bundle[image_mime], blobs)
                    blocks.append(
                        Block(
                            "image",
                            data=image if isinstance(image, bytes) else base64.b64decode(image),
                        )
                    )
                elif "text/markdown" in bundle:
                    blocks.extend(markdown_blocks(str(payload(bundle["text/markdown"], blobs))))
                elif "text/plain" in bundle:
                    blocks.append(Block("output", clean(str(payload(bundle["text/plain"], blobs)))))
                else:
                    blocks.append(
                        Block(
                            "paragraph",
                            "Rich output omitted ("
                            + ", ".join(bundle)
                            + "). Export a portable bundle to retain the original data.",
                        )
                    )
    if len(blocks) == 1:
        blocks.append(Block("paragraph", "This notebook has no cells."))
    return blocks


def pdf_bytes(blocks: list[Block]) -> bytes:
    import reportlab
    from reportlab.lib import colors
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.styles import ParagraphStyle
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.platypus import (
        Image,
        LongTable,
        Paragraph,
        SimpleDocTemplate,
        Spacer,
        TableStyle,
    )

    fonts = Path(reportlab.__file__).parent / "fonts"
    system = Path(os.environ.get("WINDIR", "/nonexistent")) / "Fonts"
    for name, candidates in {
        "KLBody": [
            system / "segoeui.ttf",
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            Path("/System/Library/Fonts/Supplemental/Arial.ttf"),
            fonts / "Vera.ttf",
        ],
        "KLMono": [
            system / "consola.ttf",
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            Path("/System/Library/Fonts/Supplemental/Courier New.ttf"),
            fonts / "Vera.ttf",
        ],
    }.items():
        cast(Any, pdfmetrics).registerFont(
            TTFont(name, str(next(path for path in candidates if path.is_file())))
        )
    buffer = io.BytesIO()
    doc = SimpleDocTemplate(
        buffer,
        pagesize=A4,
        leftMargin=45,
        rightMargin=45,
        topMargin=48,
        bottomMargin=48,
        title=blocks[0].text,
        author="KernelLedger",
    )
    styles = {
        "paragraph": ParagraphStyle(
            "Body", fontName="KLBody", fontSize=10, leading=15, spaceAfter=9, wordWrap="CJK"
        ),
        "code": ParagraphStyle(
            "Code",
            fontName="KLMono",
            fontSize=8,
            leading=12,
            backColor=colors.HexColor("#F2F4F6"),
            borderPadding=5,
            spaceAfter=2,
            wordWrap="CJK",
        ),
        "label": ParagraphStyle(
            "Label",
            fontName="KLBody",
            fontSize=8,
            leading=12,
            textColor=colors.HexColor("#505A62"),
            spaceBefore=12,
            spaceAfter=5,
        ),
        "warning": ParagraphStyle(
            "Warning",
            fontName="KLBody",
            fontSize=9,
            leading=13,
            textColor=colors.HexColor("#A1262F"),
            spaceAfter=7,
            wordWrap="CJK",
        ),
    }
    story: list[Any] = []
    for block in blocks:
        if block.kind == "image":
            image = Image(io.BytesIO(block.data))
            ratio = min(doc.width / image.imageWidth, (doc.height - 24) / image.imageHeight, 1)
            image.drawWidth, image.drawHeight = image.imageWidth * ratio, image.imageHeight * ratio
            story.extend([image, Spacer(1, 9)])
        elif block.kind == "table":
            rows = block.rows or [[""]]
            columns = max(len(row) for row in rows)
            table = LongTable(
                [[Paragraph(escape(cell), styles["paragraph"]) for cell in row] for row in rows],
                colWidths=[doc.width / columns] * columns,
                repeatRows=1,
            )
            table.setStyle(
                TableStyle(
                    [
                        ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#EBEFF4")),
                        ("VALIGN", (0, 0), (-1, -1), "TOP"),
                        ("GRID", (0, 0), (-1, -1), 0.4, colors.HexColor("#D0D6DD")),
                    ]
                )
            )
            story.extend([table, Spacer(1, 9)])
        elif block.kind in ("title", "heading"):
            level = block.level or 1
            style = ParagraphStyle(
                "Heading",
                parent=styles["paragraph"],
                fontSize=22 if block.kind == "title" else max(11, 20 - level * 2),
                leading=27 if block.kind == "title" else max(16, 26 - level * 2),
                spaceBefore=12,
                spaceAfter=10,
                keepWithNext=True,
            )
            story.append(Paragraph(escape(clean(block.text)), style))
        elif block.kind in ("code", "output"):
            style = (
                styles["code"]
                if block.kind == "code"
                else ParagraphStyle("Output", parent=styles["code"], backColor=None)
            )
            # One flowable per line lets even very long cells paginate safely.
            for line in block.text.expandtabs(4).split("\n"):
                story.append(Paragraph(escape(line).replace(" ", "&#160;") or "&#160;", style))
            story.append(Spacer(1, 7))
        else:
            story.append(
                Paragraph(
                    escape(clean(block.text)).replace("\n", "<br/>"),
                    styles.get(block.kind, styles["paragraph"]),
                )
            )

    def footer(canvas: Any, document: Any) -> None:
        canvas.saveState()
        canvas.setFont("KLBody", 8)
        canvas.setFillColor(colors.HexColor("#505A62"))
        canvas.drawString(45, 25, "KernelLedger")
        canvas.drawRightString(A4[0] - 45, 25, str(document.page))
        canvas.restoreState()

    doc.build(story, onFirstPage=footer, onLaterPages=footer)
    return buffer.getvalue()


def word_bytes(blocks: list[Block]) -> bytes:
    from docx import Document
    from docx.shared import Inches, Pt, RGBColor

    document = Document()
    document.core_properties.title = blocks[0].text
    document.core_properties.author = "KernelLedger"
    normal: Any = document.styles["Normal"]
    normal.font.name = "Calibri"
    normal.font.size = Pt(11)
    for block in blocks:
        if block.kind == "title":
            document.add_heading(clean(block.text), 0)
        elif block.kind == "heading":
            document.add_heading(clean(block.text), min(block.level, 9))
        elif block.kind == "image":
            from PIL import Image

            with Image.open(io.BytesIO(block.data)) as image:
                width, height = image.size
            ratio = min(6 / width, 8 / height)
            document.add_picture(
                io.BytesIO(block.data), width=Inches(width * ratio), height=Inches(height * ratio)
            )
        elif block.kind == "table":
            rows = block.rows or [[""]]
            table = document.add_table(rows=0, cols=max(len(row) for row in rows))
            table.style = "Light Shading Accent 1"
            for row in rows:
                cells = table.add_row().cells
                for cell, value in zip(cells, row, strict=False):
                    cell.text = clean(value)
        else:
            paragraph = document.add_paragraph()
            run = paragraph.add_run(clean(block.text))
            if block.kind in ("code", "output"):
                run.font.name = "Consolas"
                run.font.size = Pt(9)
            elif block.kind == "label":
                run.font.size = Pt(9)
                run.font.color.rgb = RGBColor.from_string("505A62")
                run.bold = True
            elif block.kind == "warning":
                run.font.color.rgb = RGBColor.from_string("A1262F")
    buffer = io.BytesIO()
    document.save(buffer)
    return buffer.getvalue()


def html_bytes(blocks: list[Block]) -> bytes:
    """Render a self-contained, offline static HTML report with zero external network assets."""
    title = escape(clean(blocks[0].text)) if blocks else "KernelLedger Notebook"
    parts: list[str] = []
    for block in blocks:
        if block.kind == "title":
            parts.append(f"<h1>{escape(clean(block.text))}</h1>")
        elif block.kind == "heading":
            lvl = max(2, min(block.level + 1, 6))
            parts.append(f"<h{lvl}>{escape(clean(block.text))}</h{lvl}>")
        elif block.kind == "label":
            parts.append(f'<div class="kl-label">{escape(clean(block.text))}</div>')
        elif block.kind == "warning":
            parts.append(f'<div class="kl-warning">{escape(clean(block.text))}</div>')
        elif block.kind == "code":
            parts.append(f'<pre class="kl-code"><code>{escape(clean(block.text))}</code></pre>')
        elif block.kind == "output":
            parts.append(f'<pre class="kl-output">{escape(clean(block.text))}</pre>')
        elif block.kind == "image":
            b64 = base64.b64encode(block.data).decode("ascii")
            parts.append(f'<figure class="kl-figure"><img src="data:image/png;base64,{b64}" alt="Figure"/></figure>')
        elif block.kind == "table":
            rows = block.rows or []
            if rows:
                header_cells = "".join(f"<th>{escape(clean(c))}</th>" for c in rows[0])
                body_rows = "".join(
                    "<tr>" + "".join(f"<td>{escape(clean(c))}</td>" for c in row) + "</tr>"
                    for row in rows[1:]
                )
                parts.append(
                    f'<div class="kl-table-wrap"><table><thead><tr>{header_cells}</tr></thead>'
                    f"<tbody>{body_rows}</tbody></table></div>"
                )
        else:
            body = escape(clean(block.text)).replace("\n", "<br/>")
            parts.append(f"<p>{body}</p>")

    html_doc = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{title}</title>
<style>
:root {{
  color-scheme: light dark;
  --bg: #ffffff; --fg: #181c20; --muted: #5a6570; --surface: #f5f7f9;
  --border: #dce2e8; --warn-bg: #fff3f3; --warn-fg: #a1262f; --accent: #2563eb;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #11151a; --fg: #e6edf3; --muted: #8b96a3; --surface: #181e26;
    --border: #2a3340; --warn-bg: #2c1619; --warn-fg: #f87171; --accent: #60a5fa;
  }}
}}
body {{
  margin: 0; padding: 2.5rem 1.5rem 4rem; background: var(--bg); color: var(--fg);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  line-height: 1.6;
}}
main {{ max-width: 56rem; margin: 0 auto; }}
h1 {{ font-size: 1.9rem; margin: 0 0 1.25rem; border-bottom: 1px solid var(--border); padding-bottom: .5rem; }}
h2, h3, h4 {{ margin-top: 1.6rem; }}
.kl-label {{ font-size: .78rem; font-weight: 600; color: var(--muted); margin: 1.1rem 0 .25rem; text-transform: uppercase; letter-spacing: .04em; }}
.kl-warning {{ font-size: .85rem; color: var(--warn-fg); background: var(--warn-bg); border: 1px solid var(--warn-fg); border-radius: 6px; padding: .5rem .75rem; margin: .4rem 0; }}
pre {{ margin: .25rem 0 .9rem; padding: .85rem 1rem; border-radius: 8px; overflow-x: auto; font-size: .86rem; line-height: 1.45; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
.kl-code {{ background: var(--surface); border: 1px solid var(--border); }}
.kl-output {{ background: transparent; border-left: 3px solid var(--border); border-radius: 0; padding-left: .9rem; }}
.kl-figure {{ margin: .75rem 0 1.25rem; }}
.kl-figure img {{ max-width: 100%; height: auto; border-radius: 6px; border: 1px solid var(--border); }}
.kl-table-wrap {{ overflow-x: auto; margin: .5rem 0 1rem; border: 1px solid var(--border); border-radius: 8px; }}
table {{ width: 100%; border-collapse: collapse; font-size: .84rem; }}
th, td {{ padding: .45rem .65rem; border-bottom: 1px solid var(--border); text-align: left; }}
th {{ background: var(--surface); font-weight: 600; }}
footer {{ margin-top: 3rem; font-size: .78rem; color: var(--muted); border-top: 1px solid var(--border); padding-top: .75rem; }}
</style>
</head>
<body>
<main>
{"".join(parts)}
<footer>Exported offline by KernelLedger</footer>
</main>
</body>
</html>
"""
    return html_doc.encode("utf-8")


def script_bytes(notebook: Notebook) -> bytes:
    """Export notebook cells as a clean runnable Python script with # %% cell markers."""
    manifest = notebook.read()
    lines: list[str] = []
    deps = manifest.get("metadata", {}).get("dependencies")
    if isinstance(deps, list) and deps and all(isinstance(d, str) for d in deps):
        lines.append("# /// script")
        lines.append("# dependencies = [")
        for dep in deps:
            lines.append(f"#   {dep!r},")
        lines.append("# ]")
        lines.append("# ///")
        lines.append("")
    title = manifest.get("metadata", {}).get("title") or notebook.root.name
    lines.append(f"# Exported from KernelLedger notebook: {title}")
    lines.append("")
    for cell in manifest["cells"]:
        ctype = cell["type"]
        cid = cell["id"]
        src = clean(notebook.source(cell)).rstrip("\n")
        meta = cell.get("metadata") if isinstance(cell.get("metadata"), dict) else {}
        disabled = bool(meta.get("disabled", False))
        if ctype == "markdown":
            lines.append(f"# %% [markdown] id={cid}")
            for ln in src.splitlines() or [""]:
                lines.append(f"# {ln}" if ln else "#")
        elif ctype == "sql":
            out_var = str(meta.get("output_var") or "_sql_df")
            lines.append(f"# %% [sql] {out_var}")
            if disabled:
                lines.append("# [disabled]")
                for ln in src.splitlines():
                    lines.append(f"# {ln}")
            else:
                lines.append(f"# SQL query bound to `{out_var}`")
                lines.append(f"_kl_sql = {src!r}")
        elif ctype == "raw":
            lines.append(f"# %% [raw] id={cid}")
            for ln in src.splitlines():
                lines.append(f"# {ln}")
        else:
            lines.append(f"# %% id={cid}")
            if disabled:
                lines.append("# [disabled]")
                for ln in src.splitlines():
                    lines.append(f"# {ln}")
            else:
                lines.append(src)
        lines.append("")
    return ("\n".join(lines).rstrip("\n") + "\n").encode("utf-8")


def export_document(
    notebook: Notebook,
    path: str,
    format: str,
    ledger: Any = None,
    blobs: Any = None,
    markdown: str | None = None,
) -> dict[str, str]:
    if format not in ("pdf", "docx", "html", "py"):
        raise SourceError("invalid_request", "Document format must be pdf, docx, html, or py")
    destination = Path(path)
    if destination.suffix.lower() != f".{format}":
        raise SourceError("invalid_request", f"Choose a file ending in .{format}")
    if format == "py":
        if markdown is not None:
            raise SourceError("invalid_request", "Python script export requires a notebook")
        atomic(destination, script_bytes(notebook))
        return {"path": str(destination), "format": format}
    blocks = (
        notebook_blocks(notebook, ledger, blobs)
        if markdown is None
        else [Block("title", "Assistant report"), *markdown_blocks(markdown)]
    )
    if format == "pdf":
        data = pdf_bytes(blocks)
    elif format == "docx":
        data = word_bytes(blocks)
    else:
        data = html_bytes(blocks)
    atomic(destination, data)
    return {"path": str(destination), "format": format}
