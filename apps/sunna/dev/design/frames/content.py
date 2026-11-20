#!/usr/bin/env python3
# Run inside the capture container. Sample planning data, not measured results.
from pathlib import Path
from xml.sax.saxutils import escape
home=Path('/home/mate/Documents')
rows=[['SUNNA · WORKSPACE STUDY','','','',''],['November / desktop capture plan','','','',''],['','','','',''],['Workspace','Width','Height','Focus','Status'],['Development',2560,1440,'Rust / X11 capture','Ready'],['Everyday',1920,1080,'Files / session notes','Ready'],['Creative',3440,1440,'Earthrise / photo study','In review'],['Documentation',1920,1080,'Browser / notes','Ready'],['Planning',2560,1600,'Workspace inventory','In progress'],['','','','',''],['REVIEW NOTES','','','',''],['Keep native panels and window decorations.','','','',''],['Use a second frame for subtle activity.','','','',''],['Check text at full resolution before export.','','','',''],['','','','',''],['Next review','Wednesday','','','']]
xml='''<?xml version="1.0" encoding="UTF-8"?>
<office:document xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.3" office:mimetype="application/vnd.oasis.opendocument.spreadsheet">
<office:automatic-styles>
<style:style style:name="wide" style:family="table-column"><style:table-column-properties style:column-width="7cm"/></style:style>
<style:style style:name="normal" style:family="table-cell"><style:text-properties fo:font-size="13pt"/></style:style>
<style:style style:name="header" style:family="table-cell"><style:table-cell-properties fo:background-color="#dce8e9"/><style:text-properties fo:font-size="14pt" fo:font-weight="bold" fo:color="#23464a"/></style:style>
<style:style style:name="row" style:family="table-row"><style:table-row-properties style:row-height="1cm"/></style:style>
</office:automatic-styles><office:body><office:spreadsheet><table:table table:name="Workspace study"><table:table-column table:style-name="wide" table:number-columns-repeated="5"/>'''
for n,row in enumerate(rows):
 xml+='<table:table-row table:style-name="row">'
 for col,value in enumerate(row):
  if n in (0,1,10,11,12,13) and col > 0:
   xml+='<table:covered-table-cell/>'; continue
  span=' table:number-columns-spanned="5"' if n in (0,1,10,11,12,13) else ''
  xml+='<table:table-cell table:style-name="'+('header' if n in (0,3,10) else 'normal')+'"'+span+' office:value-type="string"><text:p>'+escape(str(value))+'</text:p></table:table-cell>'
 xml+='</table:table-row>'
xml+='</table:table></office:spreadsheet></office:body></office:document>'
(home/'Workspace-study.fods').write_text(xml)
