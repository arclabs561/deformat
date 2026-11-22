import sys, os
OUT = sys.argv[1]
def p(n): return os.path.join(OUT, n)
from fpdf import FPDF
# Identity-H (fpdf2 embeds TTF as Type0/CIDFontType2 Identity-H with ToUnicode)
pdf = FPDF(); pdf.add_page()
pdf.add_font("DV", "", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
pdf.set_font("DV", size=12)
pdf.cell(text="Identity encoded café αβγ 中"); 
pdf.output(p("identity_h.pdf"))
# Shaped ligatures: "fi"/"fl"/"ff" shaped into ligature glyphs by harfbuzz
pdf = FPDF(); pdf.add_page()
pdf.add_font("DVS", "", "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf")
pdf.set_font("DVS", size=14)
pdf.set_text_shaping(True)
pdf.cell(text="find the flow and offer office")
pdf.output(p("shaped_lig.pdf"))
# Type3 via matplotlib
import matplotlib
matplotlib.use("pdf")
matplotlib.rcParams["pdf.fonttype"] = 3
import matplotlib.pyplot as plt
fig = plt.figure(figsize=(4, 2))
fig.text(0.1, 0.5, "Typethree glyph text")
fig.savefig(p("type3.pdf"))
print("ok")
