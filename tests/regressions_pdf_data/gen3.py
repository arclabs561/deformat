import sys, os
from reportlab.pdfgen import canvas
from reportlab.lib.pagesizes import letter
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
OUT = sys.argv[1]
pdfmetrics.registerFont(TTFont("DejaVu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
c = canvas.Canvas(os.path.join(OUT, "ttf_latin1.pdf"), pagesize=letter)
c.setFont("DejaVu", 12)
c.drawString(72, 700, "àáâãäåæçèéêëìíîïðñòóôõöøùúûüýþÿ")
c.drawString(72, 680, "Grüße aus München, £ 5 € 7")
c.showPage(); c.save()
# Same text with the standard Helvetica (WinAnsiEncoding) as a control
c = canvas.Canvas(os.path.join(OUT, "helv_latin1.pdf"), pagesize=letter)
c.setFont("Helvetica", 12)
c.drawString(72, 700, "Grüße aus München, £ 5 € 7 café")
c.showPage(); c.save()
