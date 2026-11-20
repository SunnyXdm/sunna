"""Extract exact checklist names and generate the auditable deep-link test catalog."""
from pathlib import Path
import re,json
root=Path(__file__).resolve().parents[1]
source=(root.parent/'inventory.md').read_text()
sections={}
for m in re.finditer(r'^### ([LVA]\d\d)\. (.*?)\n(.*?)(?=^### |^## |\Z)',source,re.M|re.S):
 code,title,body=m.groups(); check=re.search(r'\*\*State checklist \(\d+\):\*\* (.*)',body)
 if check: sections[code]=(title,[(int(n),re.sub(r'\s*\[[^\]]+\]\([^)]*\)','',label).strip(' ;.')) for n,label in re.findall(r'\*\*(\d+)\.\*\* (.*?)(?=; \*\*\d+\.\*\*|$)',check.group(1))])
def c(view='computer',**kw):return dict(view=view,**kw)
state=lambda s:c(state=s)
notice=lambda s:c('session',notice=s)
menu=lambda **kw:c('menu',**kw)
form=lambda s:c('add',form=s)
settings=lambda **kw:c('settings',**kw)
m={
'L01':[state('loading'),c('add',computers=0),c('overview'),state('idle'),state('checking'),state('ready-no-latency'),state('ready'),state('busy-anonymous'),state('busy'),state('key'),state('update'),state('not-found'),state('invalid'),state('unreachable'),state('offline')],
'L02':[form('idle'),c('edit',form='ready'),c('add',form='ready',link=1),form('checking'),form('ready'),form('busy'),form('key'),form('update'),form('unreachable'),form('not-found'),form('invalid'),c('add',form='ready',reveal=1),form('pending'),form('save-error')],
'L03':[c('find',discovery=s) for s in ['loading','results','empty','error','rescan','results','results','results']],
'L04':[settings(self=s) for s in ['loading','ready','busy','key','network','linux','mac']]+[settings(),settings(reveal=1),settings(permission='denied'),settings(permission='allowed'),settings(permission='reset'),settings(toast='io: permission denied',saveError=True)],
'L05':[c('qr',qr=s) for s in ['generating','ready','error']],
'L06':[c(remove=True),c(),c(removed=True,toast='Removed archlinux.'),c(remove=True,removeError=True,toast='io: permission denied')],
'L07':[c(permission='denied',permissionShown=True),settings(permission='opened'),settings(permission='allowed'),settings(permission='reset'),settings(permission='reset-failed')],
'L08':[c(context=True),c(state='offline',context=True),c()],
'L09':[c('connecting',phase='reaching'),c('connecting',phase='video'),c('session',notice='Connected · 12 ms'),c(ended='unexpected'),c(),c(fail='spawn')],
'L10':[c(toast='Still looking for archlinux…'),c(toast='io: permission denied'),c(toast='Copied.'),c(toast="archlinux isn't responding.",toastAction='edit'),c()],
'V01':[c('connecting',phase='video'),c('session'),c('session',fullscreen=False),c('session'),c('session'),c('session',pointer='outside'),c('session',stats=False,menu=False),c(ended='host')],
'V02':[c('session',stats=False),c('session',phase='waiting',stats=True),c('session',statsDetail='basic',stats=True)]+[c('session',stats=True) for _ in range(7)],
'V03':[c('session')]+[notice(s) for s in ['help','unavailable','captured','released','changing','changed','stream-error','clipboard-on','clipboard-off','clipboard-empty','skipped','sent','sound-on','sound-off','hidden','no-drawing','no-video']],
'V04':[c('session'),c('session'),menu(),c('session',menu=False)],
'V05':[c('session'),menu(phase='waiting'),menu(),menu(fullscreen=True),menu(stats=True),menu(sound=True)],
'V06':[menu(submenu='keyboard',capture=True),menu(submenu='keyboard',capture=False),menu(submenu='keyboard',permission='denied'),menu(submenu='keyboard',sel=1),menu(submenu='keyboard',sel=0)],
'V07':[menu(submenu='clipboard',clipboard=True),menu(submenu='clipboard',clipboard=False),notice('clipboard-empty'),notice('typing'),notice('skipped')],
'V08':[menu(submenu='video',phase='waiting'),menu(submenu='video'),notice('changing'),notice('changed'),notice('stream-error')],
'V09':[c('session'),c('session',pointer='outside'),menu(),c('session',pointer='fallback'),c('session',zoom=1.3)],
'A01':[state('loading'),c('add',computers=0),c('overview'),state('checking'),state('ready-no-latency'),state('ready'),state('busy-anonymous'),state('busy'),state('key'),state('update'),state('not-found'),state('invalid'),state('unreachable'),state('offline')],
'A02':[form('idle'),c('edit',form='ready'),c('add',form='ready',link=1),c('add',form='ready',link=1),form('idle'),form('checking'),form('ready'),form('busy'),form('key'),form('update'),form('not-found'),form('invalid'),form('unreachable'),c('add',form='ready',reveal=1),form('invalid-key'),form('save-error')],
'A03':[c(context=True),c(context=True,state='offline'),c()],
'A04':[c(remove=True),c(),c(removed=True),c(remove=True,removeError=True,toast="Couldn't remove it: the phone's storage may be full.")],
'A05':[settings(),settings(focus='ownName'),settings(sound=False,suggestions=True),c()],
'A06':[c('licenses'),c('licenses',licensesEmpty=True),settings()],
'A07':[c('connecting',phase='reaching'),c('connecting',phase='video'),c('session'),c(),c(fail='unreachable'),c(ended='host')],
'A08':[c('session'),c('session',notification='background'),c('session',touch='Trackpad'),c('session',touch='Touch'),c('session',zoom=1.5),c('session',keyboard=True),menu(),c(ended='host')],
'A09':[c('session'),c('session',keyboardButton=False),c('session',keyboard=True),menu(),c('session',idle=True),c('session',floatX=230)],
'A10':[c('session',stats=False),c('session',stats=True,phase='waiting'),c('session',stats=True,statsDetail='basic'),c('session',stats=True),c('session',stats=True,decoder='hardware'),c('session',stats=True,decoder='software')],
'A11':[menu(),menu(touch='Touch'),menu(touch='Trackpad'),menu(sound=True),menu(sound=False),menu(stats=True),menu(stats=False),menu(submenu='clipboard',clipboard=True),menu(submenu='clipboard',clipboard=False),menu(submenu='video',resolution='75%'),menu(submenu='video',notice='stream-error',streamError=True)],
'A12':[c('session'),c('session',keyboard=True,suggestions=False),c('session',keyboard=True,suggestions=True),c('session',keyboard=True),c('session',keyboard=True,modifier='armed'),c('session',keyboard=True,modifier='locked'),c('session',keyboard=True,notice='unsupported')],
'A13':[c('session',pointer='fallback'),c('session'),c('session',pulse=True),c('session',zoom=1.5),c('session',pointer='fallback')],
'A14':[c(toast='Copied 100.101.102.103'),c(toast="archlinux isn't responding.",toastAction='edit'),notice('help'),c(fail='unreachable'),notice('typing'),notice('unsupported'),notice('stream-error')],
'A15':[c('connecting',notification='starting',testOpen=True),c('session',notification='connected',testOpen=True),c('session',notification='background',testOpen=True),c('session',notification='denied',testOpen=True),c(notification='disconnected',testOpen=True)],
'A16':[settings(permission='not-requested',permissionShown=True),settings(permission='opened',permissionShown=True),settings(permission='allowed',permissionShown=True),settings(permission='denied',permissionShown=True)],
'A17':[form('idle'),form('ready'),form('ready'),c('session',keyboard=True),menu(),menu(),c()]
}
for item,detail in zip(m['V02'][3:],['host','latency','decoder','rtt','loss','sound','full']):item['statsDetail']=detail
m['V04'][1]['menuHover']=True
for item in m['A16']:item['scroll']=500
m['L03'][6]['scroll']=230
m['L03'][7]['scroll']=230
m['V03'][15]['menu']=False
for code in ['L04','L07']:
 for item in m[code]:
  if item.get('permission') in ['denied','opened','reset','reset-failed']:item['scroll']=250
omissions={
'V01.7':'Native CPU fallback renderer is not implemented; representative screen with overlays hidden only. This browser draws HTML stand-ins.',
'V09.1':'Arbitrary remote RGBA cursor transport is not implemented; a drawn arrow represents the pointer.',
'V09.4':'Native cursor fallback is represented by the same drawn arrow; no native cursor API.',
'V09.5':'Native cursor bitmap scaling is not implemented; local picture zoom represents the visual state.',
'A08.2':'No native background service in a static browser prototype; notification example is available in Test.',
'A13.2':'Arbitrary host cursor bitmap decoding is not implemented; drawn arrow represents it.',
'A13.5':'Invalid host bitmap handling is not implemented; fallback arrow represents it.',
'A17.4':'The real system IME is not raised. A drawn keyboard is used in sessions as requested; form inputs use the browser keyboard when tapped.',
'A17.5':'Native sheet drag-to-dismiss was replaced by the guide’s in-place views and half-step-out menu; Back/Close is available.',
'A17.6':'Native cancelled sheet drag is not in the prototype because guide views replace that sheet shell.'
}
cases=[]
for code,(title,states) in sections.items():
 assert len(m[code])==len(states),(code,len(m[code]),len(states))
 for (n,label),config in zip(states,m[code]):
  id=f'{code}.{n}';cases.append(dict(id=id,surface=title,label=label,config=config,limitation=omissions.get(id,'')))
(root/'tools/cases.json').write_text(json.dumps(cases,indent=2))
runtime=(root/'test.js').read_text().split('const CFG_KEYS=',1)[1] if (root/'test.js').exists() and 'const CFG_KEYS=' in (root/'test.js').read_text() else ''
(root/'test.js').write_text('// Every numbered inventory checklist state. Generated by tools/catalog.py.\nconst CASES = '+json.dumps(cases,indent=2)+';\n'+('const CFG_KEYS='+runtime if runtime else ''))
print(len(cases),'checklist cases')
