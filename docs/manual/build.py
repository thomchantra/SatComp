#!/usr/bin/env python3
"""Build the manual PDFs: docs/manual/<Plugin>.md -> docs/manual/<Plugin>-Manual-v<version>.pdf

A4, solid black pages, white Fira Code. Needs `pip install weasyprint markdown` (and pango: `brew install pango`;
on macOS run with DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib). Usage: build.py [Plugin ...]  (default: all)
The section "## Notes to check ..." is the author's review list and is dropped from the PDF.
"""
import pathlib, re, sys
import markdown
from weasyprint import HTML

HERE = pathlib.Path(__file__).resolve().parent
FONTS = HERE.parent.parent / "Fonts" / "Fira_Code_v6.2" / "ttf"

CSS = """
@font-face { font-family: 'Fira Code'; font-weight: 400; src: url('%(f)s/FiraCode-Regular.ttf'); }
@font-face { font-family: 'Fira Code'; font-weight: 700; src: url('%(f)s/FiraCode-Bold.ttf'); }
@page {
  size: A4; margin: 18mm 16mm 20mm 16mm; background: #000;
  @bottom-left { content: '%(title)s %(version)s'; font: 7.5pt 'Fira Code'; color: #888; }
  @bottom-right { content: counter(page); font: 7.5pt 'Fira Code'; color: #888; }
}
@page :first { @bottom-left { content: ''; } @bottom-right { content: ''; } }
html { background: #000; color: #fff; font: 8.6pt/1.5 'Fira Code', monospace; }
body { margin: 0; }
h1 { font-size: 22pt; margin: 0 0 4mm; padding: 2mm 3mm; background: #fff; color: #000; }
h2 { font-size: 11.5pt; margin: 8mm 0 2.5mm; padding-bottom: 1mm; border-bottom: 1.5px solid #fff; page-break-after: avoid; }
h3 { font-size: 9.6pt; margin: 5mm 0 1.5mm; page-break-after: avoid; }
h3::before { content: '> '; }
p { margin: 0 0 2.5mm; }
em { font-style: normal; text-decoration: underline; }
strong { font-weight: 700; }
a { color: #fff; }
img { display: block; max-width: 100%%; max-height: 118mm; width: auto; height: auto; margin: 4mm auto; border: 1px solid #555; }
code { font: inherit; border: 1px solid #555; padding: 0 1mm; }
pre { border: 1px solid #fff; padding: 2.5mm 3mm; margin: 2mm 0 4mm; white-space: pre-wrap; page-break-inside: avoid; }
pre code { border: 0; padding: 0; }
ul, ol { margin: 0 0 3mm; padding-left: 5mm; }
li { margin-bottom: 1mm; }
table { width: 100%%; border-collapse: collapse; margin: 2mm 0 4mm; font-size: 8pt; }
th { background: #fff; color: #000; text-align: left; padding: 1mm 2mm; }
td { border: 1px solid #555; padding: 1mm 2mm; vertical-align: top; }
tr { page-break-inside: avoid; }
td:first-child { white-space: nowrap; font-weight: 700; }
.tagline { color: #bbb; margin: -2mm 0 4mm; }
"""

def split_front(text):
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    meta = dict(l.split(": ", 1) for l in m.group(1).splitlines()) if m else {}
    return meta, text[m.end():] if m else text

def build(md_path):
    meta, body = split_front(md_path.read_text())
    body = re.split(r"\n## Notes to check", body)[0]
    html = markdown.markdown(body, extensions=["tables", "fenced_code"])
    # the tagline goes right under the first heading
    html = html.replace("</h1>", "</h1>\n<p class='tagline'>%s</p>" % meta.get("tagline", ""), 1)
    css = CSS % {"f": FONTS.as_uri(), "title": meta["title"], "version": "v" + meta["version"]}
    out = md_path.with_name("%s-Manual-v%s.pdf" % (meta["title"], meta["version"]))
    HTML(string="<html><body>%s</body></html>" % html, base_url=str(md_path.parent) + "/").write_pdf(out, stylesheets=[__import__("weasyprint").CSS(string=css)])
    print("wrote", out.name)

if __name__ == "__main__":
    names = sys.argv[1:] or ["SatComp", "SatXComp", "SatLim"]
    for n in names:
        build(HERE / (n + ".md"))
