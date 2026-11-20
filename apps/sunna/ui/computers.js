// The approved desktop compositions, with data and actions supplied by the launcher.
import { animate, springs, reducedMotion } from './motion.js';
export const VIEWS = ['devices', 'wall', 'space', 'type', 'gallery'];
const $ = (s, r) => r.querySelector(s);
const all = (s, r) => [...r.querySelectorAll(s)];
const node = (tag, cls, text) => { const el = document.createElement(tag); el.className = cls; if (text != null) el.textContent = text; return el; };
export function copyScreen(source) {
  const copy = source.cloneNode(true);
  all('canvas', source).forEach((canvas, i) => all('canvas', copy)[i].getContext('2d').drawImage(canvas, 0, 0));
  return copy;
}
const position = (el, x, y, w, h) => Object.assign(el.style, { left: `${x}px`, top: `${y}px`, width: `${w}px`, ...(h == null ? {} : {height: `${h}px`}) });

// The prototype's exhaustive row scoring for small collections; bounded rows
// for large libraries keep every screen readable and the layout linear-time.
function rowsFor(list, W, H, gallery) {
  if (list.length > 9) {
    const columns = Math.max(1, Math.floor(W / (gallery ? 320 : 350)));
    return Array.from({length: Math.ceil(list.length / columns)}, (_, i) => list.slice(i*columns, (i+1)*columns));
  }
  let best;
  for (let mask = 0; mask < 2 ** (list.length-1); mask++) {
    const rows = [[list[0]]];
    for (let i=1;i<list.length;i++) mask & (1 << (i-1)) ? rows.push([list[i]]) : rows.at(-1).push(list[i]);
    if (gallery && rows.some((r,i) => i && r.length < rows[i-1].length)) continue;
    const gap = gallery ? 22 : 2;
    const total = rows.reduce((sum,r) => sum+(W-gap*(r.length-1))/r.reduce((a,c)=>a+c.ratio,0)+(gallery?50:0),0)+gap*(rows.length-1);
    const edge = rows.slice(1).some(r => r[0].state !== 'ready' && list[list.indexOf(r[0])-1].state === 'ready');
    const score = Math.abs(Math.log(H/total)) + (gallery ? (edge?-.25:0) : list.length>2 ? .2*rows.filter(r=>r.length===1).length:0);
    if (!best || score < best.score) best = {rows, score};
  }
  return best.rows;
}

export function computerViews(host, hooks) {
  let view = 'devices', list = [], selected = null, signature = '', root, cards = new Map(), raf;
  let tx=0,ty=0,cx=0,cy=0;
  const pick = (id, focus = false) => {
    if (!cards.has(id)) return;
    const changed = selected !== id; selected = id;
    if (view === 'devices') layout();
    cards.forEach((card,key) => card.classList.toggle('on', key === selected));
    if (root) all('.amb > *',root).forEach(c=>c.classList.toggle('on',c.dataset.id===selected));
    if (view === 'type') all('nav button',root).forEach(c=>c.classList.toggle('on',c.dataset.id===selected));
    if (changed && view === 'devices' && !reducedMotion()) animate($('.cap',root),[{opacity:0,transform:'translateY(8px)'},{opacity:1,transform:'none'}],{duration:300,easing:'ease-out'});
    if (focus) {
      const hit = view === 'type' ? $(`nav button[data-id="${id}"]`,root) : $('.tile-hit',cards.get(id));
      hit?.focus({preventScroll:true});
      if (!['devices','type'].includes(view)) cards.get(id).scrollIntoView({block:'nearest'});
      if(view==='type') hit.scrollIntoView({block:'nearest'});
    }
  };
  function screen(c) {
    const scr = node('div', `scr screen ${c.state} ${c.picture?'has-picture':''}`);
    scr.style.aspectRatio = String(c.ratio);
    if (c.picture) {
      const canvas = node('canvas', c.picture.width <= 64 ? 'saved-picture soft' : 'saved-picture'); canvas.width=c.picture.width; canvas.height=c.picture.height;
      canvas.getContext('2d').drawImage(c.picture,0,0); scr.append(canvas);
    } else scr.append(hooks.standin(c.machine, c.check));
    return scr;
  }
  function updateLabel(c, cls='computer-update') {
    const el=node('span',cls,c.update.label);el.title=c.update.detail;
    return el;
  }
  function label(c, cls='lab') {
    const lab=node('div',cls);lab.append(node('b','computer-name',c.name));
    const status=node('span','computer-status'); status.append(node('i',`dot ${c.state}${c.check?.unverified ? ' unverified' : ''}`),document.createTextNode(c.state === 'not-sharing' && c.detail ? c.status + ' · ' + c.detail : c.status));lab.append(status);if(c.update)lab.append(updateLabel(c));return lab;
  }
  function unit(c) {
    const u=node('div','unit'),b=node('div','bezel');b.append(screen(c));u.append(b);
    if(c.kind==='laptop') u.append(node('div','deck'));
    else if(['desktop','mini'].includes(c.kind)) {u.append(node('div','neck'),node('div','foot'));if(c.kind==='mini') u.append(node('div','mini-box'));}
    else if(['vm','server'].includes(c.kind)) u.append(node('div',`rack ${c.kind}`));
    return u;
  }
  function card(c) {
    const el=node('div',`computer ${view==='devices'?'dev '+c.kind:view==='space'?'pan':view==='type'?'pic':'tile'}`);
    el.dataset.id=c.id;el.dataset.os=c.osKind;el.dataset.state=c.state;el.setAttribute('role','listitem');
    const hit=node('button','tile-hit');hit.type='button';hit.setAttribute('aria-label',`${c.name}. ${c.status}${c.update?'. '+c.update.label+'. '+c.update.detail:''}`);
    if(view==='devices') {
      hit.append(unit(c));const refl=node('div','refl');refl.setAttribute('aria-hidden','true');refl.append(unit(c));const tag=node('div','tag',c.name);if(c.update)tag.append(updateLabel(c));el.append(refl,tag);
      if(c.picture && c.state!=='off'){const spill=node('div','spill');spill.append(screen(c));el.append(spill);}
    } else if(view==='space') {const pn=node('div','pn'),fl=node('div','fl');fl.style.animationDelay=`${-list.indexOf(c)*1.7}s`;fl.append(screen(c),label(c,'lb'));pn.append(fl);hit.append(pn);}
    else {hit.append(screen(c)); if(view!=='type')hit.append(label(c));}
    const more=node('button','computer-more','•••');more.type='button';more.title=`More — ${c.name}`;more.setAttribute('aria-label',`More — ${c.name}`);
    more.onclick=e=>{e.stopPropagation();hooks.menu(c.id,{anchor:more});};
    more.onfocus=()=>{if(view==='type')pick(c.id);};
    el.oncontextmenu=e=>{e.preventDefault();hooks.menu(c.id,{x:e.clientX,y:e.clientY});};
    hit.onclick=()=>{if(view==='devices'&&selected!==c.id)pick(c.id);else hooks.activate(c.id);};
    hit.onfocus=()=>pick(c.id);
    if(view==='wall') {el.onpointerenter=()=>{root.classList.add('hov');cards.forEach((e,id)=>e.classList.toggle('on',id===c.id));};el.onpointerleave=()=>root.classList.remove('hov');const hint=node('div','hint',c.state==='ready'?'Connect':c.state==='key' && c.check?.state!=='identity-changed'?'Fix the key…':'Details');el.append(hint);}
    if(view==='type') {const p=node('p','type-detail',c.detail+' · '+c.status);if(c.update)p.append(updateLabel(c));hit.append(p);}
    el.append(hit,more);if(c.error&&view!=='devices'){const error=node('div','computer-error',c.error);error.setAttribute('role','status');el.append(error);}return el;
  }
  function build() {
    cancelAnimationFrame(raf);host.replaceChildren();cards.clear();
    root=node('section',`computer-view L${VIEWS.indexOf(view)+1} placing`);root.id='computers';host.append(root);
    const grid=node('div',view==='devices'?'stage':view==='space'?'scene':view==='type'?'pics':'view-grid');grid.id='grid';grid.setAttribute('role','list');grid.setAttribute('aria-label','Your computers');root.append(grid);
    if(!list.length){hooks.empty(root);root.classList.remove("placing");return;}
    if(!list.some(c=>c.id===selected))selected=list[view==='devices'&&list.length>1?1:0].id;
    if(['devices','type'].includes(view)){const amb=node('div','amb');for(const c of list.filter(c=>c.picture&&c.state!=='off')) {const s=screen(c);s.dataset.id=c.id;amb.append(s);}root.prepend(amb);}
    if(view==='devices')root.prepend(node('div','desk'));
    const world=view==='space'?node('div','world'):grid;if(view==='space')grid.append(world);
    for(const c of list){const el=card(c);cards.set(c.id,el);world.append(el);}
    if(view==='devices'){
      root.append(node('div','cap'));const pips=node('div','pips');
      list.forEach(c=>{const p=node('button','pip');p.title=c.name;p.setAttribute('aria-label',`Select ${c.name}`);p.onclick=()=>pick(c.id,true);p.dataset.id=c.id;pips.append(p);});root.append(pips);
      let down=null, dragged=false, cool=0;
      grid.onpointerdown=e=>{down=e.clientX;dragged=false;};
      grid.onpointerup=e=>{if(down!=null&&Math.abs(e.clientX-down)>36){dragged=true;step(e.clientX<down?1:-1);}down=null;};
      grid.addEventListener('click',e=>{if(dragged){e.preventDefault();e.stopPropagation();dragged=false;}},true);
      grid.onwheel=e=>{const d=Math.abs(e.deltaX)>Math.abs(e.deltaY)?e.deltaX:e.deltaY;if(Math.abs(d)>12&&Date.now()>cool){cool=Date.now()+320;step(d>0?1:-1);}};
    }
    if(view==='type'){
      const nav=node('nav','type-nav');nav.setAttribute('aria-label','Computers');
      list.forEach(c=>{const b=node('button','type-name');b.dataset.id=c.id;b.title=`${c.name} · ${c.status} · ${c.detail}`;b.append(node('b','',c.name));const sup=node('sup','');sup.append(node('i',`dot ${c.state}${c.check?.unverified ? ' unverified' : ''}`),document.createTextNode(c.brief ?? c.status));if(c.update)sup.append(updateLabel(c));b.append(sup);b.onpointerenter=()=>pick(c.id);b.onfocus=()=>pick(c.id);b.onclick=()=>hooks.activate(c.id);b.oncontextmenu=e=>{e.preventDefault();hooks.menu(c.id,{anchor:b});};nav.append(b);});root.append(nav);
      const footer=node('footer','');footer.append(node('b','','Sunna'),node('span','',hooks.summary()));root.append(footer);
    }
    if(view==='space'){
      root.onpointermove=e=>{if(!reducedMotion()){tx=e.clientX/host.clientWidth-.5;ty=e.clientY/host.clientHeight-.5;}};root.onpointerleave=()=>{tx=ty=0;};
      const tick=()=>{cx+=(tx-cx)*.07;cy+=(ty-cy)*.07;world.style.transform=`rotateY(${cx*9}deg) rotateX(${-cy*5}deg)`;raf=requestAnimationFrame(tick);};if(!reducedMotion())tick();
    }
    layout();pick(selected);
    // Establish positions before enabling transitions. Polls must not animate
    // a fresh set of cards out of the same default position.
    void root.offsetHeight;root.classList.remove("placing");
  }
  function step(n) {pick(list[Math.max(0,Math.min(list.length-1,list.findIndex(c=>c.id===selected)+n))]?.id,true);}
  function layout() {
    if(!list.length)return;
    const W=host.clientWidth,H=host.clientHeight;
    if(view==='devices') {
      const sel=Math.max(0,list.findIndex(c=>c.id===selected));
      const sizes=list.map(c=>{const extra=c.kind==='laptop'?23:['desktop','mini'].includes(c.kind)?55:['server','vm'].includes(c.kind)?47:12;const maxH=Math.min(H*.46,H*.62-64-extra);let w=W*.52,h=w/c.ratio;if(h>maxH){h=maxH;w=h*c.ratio;}return {w,h};});
      const scale=.56,gap=W<800?32:56,x=[];x[sel]=0;
      for(let i=sel+1;i<list.length;i++)x[i]=x[i-1]+sizes[i-1].w*(i-1===sel?1:scale)/2+gap+sizes[i].w*scale/2;
      for(let i=sel-1;i>=0;i--)x[i]=x[i+1]-sizes[i+1].w*(i+1===sel?1:scale)/2-gap-sizes[i].w*scale/2;
      list.forEach((c,i)=>{const d=cards.get(c.id);d.style.width=`${sizes[i].w}px`;d.style.setProperty('--h',`${sizes[i].h}px`);d.style.transform=`translateX(${x[i]-sizes[i].w/2}px) scale(${i===sel?1:scale})`;d.classList.toggle('on',i===sel);$('.tile-hit',d).tabIndex=i===sel?0:-1;$('.computer-more',d).tabIndex=i===sel?0:-1;});
      const c=list[sel],cap=$('.cap',root);root.classList.toggle('long-name',c.name.length>30);cap.replaceChildren(node('h1','',c.name));const p=node('p','');const st=node('span','st');st.append(node('i',`dot ${c.state}${c.check?.unverified ? ' unverified' : ''}`),document.createTextNode(c.status));p.append(st,node('span','',c.detail));cap.append(p);
      const go=node('button',`go ${c.state==='ready'?'':'alt'}`,c.state==='ready'?'Connect ↵':c.state==='key' && c.check?.state!=='identity-changed'?'Fix the key…':'Details');go.onclick=()=>hooks.activate(c.id);cap.append(go);if(c.update){const update=node('button','computer-update update-link',c.update.label);update.title=c.update.detail;update.onclick=()=>hooks.update(c.id,update);cap.append(update);}
      if(c.error)cap.append(node('div','computer-error',c.error));
      all('.pip',root).forEach(p=>{p.classList.toggle('on',p.dataset.id===selected);p.setAttribute('aria-pressed',String(p.dataset.id===selected));});
    } else if(view==='wall'||view==='gallery') {
      const gallery=view==='gallery',G=gallery?22:2,top=gallery?68:0,bottom=gallery?30:0,pad=gallery?44:0,LAB=gallery?(list.some(c=>c.update)?70:50):0;
      const available=H-top-bottom;let cw=W-pad*2;
      const rows=rowsFor(list,cw,available,gallery);
      const total=w=>rows.reduce((sum,r)=>sum+(w-G*(r.length-1))/r.reduce((s,c)=>s+c.ratio,0)+LAB,0)+G*(rows.length-1);
      if(gallery&&list.length<=9&&total(cw)>available){let lo=Math.min(300,cw),hi=cw;for(let k=0;k<24;k++){const mid=(lo+hi)/2;total(mid)>available?hi=mid:lo=mid;}cw=lo;}
      const factor=gallery||list.length>9?1:Math.max(.7,Math.min(1.4,(H-G*(rows.length-1))/(total(cw)-G*(rows.length-1))));
      let y=top+(gallery&&list.length<=9?Math.max(0,(available-total(cw))/2):0);
      rows.forEach(r=>{const hn=(cw-G*(r.length-1))/r.reduce((s,c)=>s+c.ratio,0),h=hn*factor;let x=(W-cw)/2;
        r.forEach(c=>{const el=cards.get(c.id),w=c.ratio*hn;position(el,x,y,w,gallery?null:h);el.classList.toggle('big',w>420);x+=w+G;});y+=h+LAB+G;});
      let end=$('.scroll-end',root);if(!end){end=node('i','scroll-end');$('.view-grid',root).append(end);}end.style.top=`${y+bottom}px`;
    } else if(view==='space') {
      const pw=Math.min(W*.26,H*.46),rows=[];
      let arranged=list;
      if(list.length<=6){const ready=list.filter(c=>c.state==='ready').sort((a,b)=>(b.kind==='laptop')-(a.kind==='laptop'));const front=[...ready,...list.filter(c=>!ready.includes(c))].slice(0,3);arranged=[...front,...list.filter(c=>!front.includes(c))];}
      for(let i=0;i<arranged.length;i+=3)rows.push(arranged.slice(i,i+3));
      const world=$('.world',root);world.style.height=`${Math.max(H, rows.length>2?rows.length*(pw*.8+80)+100:H)}px`;
      const row=(items,y,z)=>{const R=pw*4,gap=pw*.14,widths=items.map(c=>c.ratio>2?pw*1.34:pw);const fit=Math.min(1,W*.84/(widths.reduce((a,b)=>a+b,0)+gap*(items.length-1)));widths.forEach((w,i)=>widths[i]=w*fit);let s=-(widths.reduce((a,b)=>a+b,0)+gap*(items.length-1))/2;
        items.forEach((c,i)=>{const el=cards.get(c.id),th=(s+widths[i]/2)/R;s+=widths[i]+gap;const depth=rows.length>2?0:z+R*(1-Math.cos(th)),project=1500/(1500-depth);el.style.width=`${widths[i]*project}px`;el.style.left=`${W/2+R*Math.sin(th)*project-widths[i]*project/2}px`;el.style.top=`${rows.length>2?y:H*.4+(H*.2+y)*project-el.offsetHeight/2}px`;el.style.setProperty('--r',`${-th*180/Math.PI}deg`);el.style.zIndex=String(z<0?1:2);});};
      if(rows.length<=2){if(rows[1])row(rows[1],-pw*.98,-pw*1.5);row(rows[0],0,0);}else{root.classList.add('many');rows.forEach((r,i)=>row(r,100+i*(pw*.8+80),0));}
    } else {
      const left=W*.47;root.style.setProperty('--fs',`${Math.max(40,Math.min(78,(H-190)/Math.min(list.length,6)/1.08,W*.058))}px`);
      list.forEach(c=>{let h=H*.62,w=h*c.ratio;const cap=(W-left)*1.26;if(w>cap){w=cap;h=w/c.ratio;}position(cards.get(c.id),left,(H-h)/2-12,w,h);});
    }
  }
  const resize=new ResizeObserver(()=>{if(!hooks.busy())layout();});resize.observe(host);
  return {
    render(nextView, nextList) {
      const active=document.activeElement,focused=active?.closest('[data-id]')?.dataset.id,more=active?.classList.contains('computer-more');
      const nextSignature=nextView+JSON.stringify(nextList.map(c=>[c.id,c.name,c.kind,c.ratio,c.state,c.status,c.detail,c.error,c.update,c.pictureVersion,!!c.picture]));
      list=nextList;view=nextView;
      if(view==='gallery'){const rank={ready:0,busy:1,off:2,key:3};list=[...list].sort((a,b)=>(rank[a.state]??4)-(rank[b.state]??4)||(a.check?.rtt_ms??0)-(b.check?.rtt_ms??0));}
      if(signature!==nextSignature){const scroller=root?.querySelector('.view-grid,.scene,.type-nav');const scroll=scroller?.scrollTop||0;signature=nextSignature;build();const nextScroller=root?.querySelector('.view-grid,.scene,.type-nav');if(nextScroller)nextScroller.scrollTop=scroll;if(focused&&cards.has(focused)){pick(focused);(more?$('.computer-more',cards.get(focused)):view==='type'?$(`nav button[data-id="${focused}"]`,root):$('.tile-hit',cards.get(focused)))?.focus({preventScroll:true});}}
      hooks.tiles(cards);
    },
    select: pick,
    keys(event) {
      if(!list.length)return false;
      if(event.key.startsWith('Arrow')) {step(['ArrowLeft','ArrowUp'].includes(event.key)?-1:1);return true;}
      if(event.key==='Enter'&&document.activeElement===document.body){hooks.activate(selected);return true;}
      return false;
    },
    selected:()=>selected,
  };
}
