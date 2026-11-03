import io, sys, os
from reportlab.pdfgen import canvas
from reportlab.lib.pagesizes import letter
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from pypdf import PdfReader, PdfWriter
from pypdf.generic import RectangleObject

OUT = sys.argv[1]
def p(name): return os.path.join(OUT, name)

# 1 simple
c = canvas.Canvas(p("simple.pdf"), pagesize=letter)
c.setFont("Helvetica", 12); c.drawString(72, 700, "Hello world"); c.showPage(); c.save()

# 2 two columns
c = canvas.Canvas(p("twocol.pdf"), pagesize=letter)
c.setFont("Helvetica", 11)
words = ["one", "two", "three", "four", "five"]
for i, w in enumerate(words):
    c.drawString(72, 700 - 14 * i, f"Left {w} alpha")
    c.drawString(320, 700 - 14 * i, f"Right {w} beta")
c.showPage(); c.save()

# 3 hyphenation: hard hyphen at EOL and soft hyphen at EOL
c = canvas.Canvas(p("hyphen.pdf"), pagesize=letter)
c.setFont("Helvetica", 12)
c.drawString(72, 700, "The result was extra-")
c.drawString(72, 686, "ordinary indeed.")
c.drawString(72, 650, "A soft hyphen splits infor­")
c.drawString(72, 636, "mation here.")
c.showPage(); c.save()

# 4 words positioned separately without space characters
c = canvas.Canvas(p("nospace.pdf"), pagesize=letter)
c.setFont("Helvetica", 12)
x = 72
for w in ["Separately", "placed", "words"]:
    c.drawString(x, 700, w); x += c.stringWidth(w, "Helvetica", 12) + 4
c.showPage(); c.save()

# 5 form xobject
c = canvas.Canvas(p("form.pdf"), pagesize=letter)
c.beginForm("F1")
c.setFont("Helvetica", 12); c.drawString(72, 600, "Inside the form xobject")
c.endForm()
c.setFont("Helvetica", 12); c.drawString(72, 700, "Outside text")
c.doForm("F1")
c.showPage(); c.save()

# 6 empty middle page
c = canvas.Canvas(p("emptymid.pdf"), pagesize=letter)
c.setFont("Helvetica", 12); c.drawString(72, 700, "Page one text"); c.showPage()
c.showPage()
c.setFont("Helvetica", 12); c.drawString(72, 700, "Page three text"); c.showPage()
c.save()

# 7 rotated page (Rotate 90) built from simple
r = PdfReader(p("simple.pdf")); w = PdfWriter()
for pg in r.pages:
    pg.rotate(90); w.add_page(pg)
w.write(p("rotated.pdf"))

# 8 encrypted (user password) AES-256 and RC4-128; owner-only
for alg, name in [("AES-256", "enc_aes256.pdf"), ("RC4-128", "enc_rc4.pdf"), ("AES-128", "enc_aes128.pdf")]:
    r = PdfReader(p("simple.pdf")); w = PdfWriter()
    for pg in r.pages: w.add_page(pg)
    w.encrypt(user_password="secret", owner_password="owner", algorithm=alg)
    w.write(p(name))
r = PdfReader(p("simple.pdf")); w = PdfWriter()
for pg in r.pages: w.add_page(pg)
w.encrypt(user_password="", owner_password="owner", algorithm="AES-256")
w.write(p("enc_owner_only.pdf"))

# 9 mediabox with offset origin
r = PdfReader(p("simple.pdf")); w = PdfWriter()
for pg in r.pages:
    w.add_page(pg)
w.pages[0].mediabox = RectangleObject([50, 600, 400, 800])
w.write(p("mediabox_offset.pdf"))

# 10 TTF font (reportlab subset, ligature codepoints present in text)
pdfmetrics.registerFont(TTFont("DejaVu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
c = canvas.Canvas(p("ttf_lig.pdf"), pagesize=letter)
c.setFont("DejaVu", 12); c.drawString(72, 700, "ﬁnd the ﬂow and oﬀer"); c.showPage(); c.save()
c = canvas.Canvas(p("ttf_plain.pdf"), pagesize=letter)
c.setFont("DejaVu", 12); c.drawString(72, 700, "Café naïve über 中"); c.showPage(); c.save()

# 11 three-page doc where page 2 has text, for segments page numbers
print("ok")
