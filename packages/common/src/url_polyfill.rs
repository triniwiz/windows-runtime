//! JS `URL` / `URLSearchParams` polyfill for standalone engines that lack them (QuickJS, Hermes,
//! JSC, V8-standalone; Node/Bun/Deno provide their own so it self-skips). It's backed by the native
//! `__urlParse` / `__urlWith` helpers installed by `runtime::napi_engine::globals::install_globals`
//! (WHATWG parsing via the Rust `url` crate). A host runs [`POLYFILL`] once after bring-up.

/// Evaluate this once (via the host's script runner) to install `URL`/`URLSearchParams`.
pub const POLYFILL: &str = r#"(function(g){
  if (typeof g.URL !== 'undefined') return;
  var P = g.__urlParse, W = g.__urlWith;
  function enc(s){ return encodeURIComponent(s).replace(/%20/g,'+'); }
  function dec(s){ return decodeURIComponent(String(s).replace(/\+/g,' ')); }
  function parseQuery(q){ var out=[]; q=String(q||''); if(q.charAt(0)==='?')q=q.slice(1);
    if(!q) return out;
    q.split('&').forEach(function(p){ if(!p) return; var i=p.indexOf('=');
      if(i<0) out.push([dec(p),'']); else out.push([dec(p.slice(0,i)),dec(p.slice(i+1))]); });
    return out; }
  function USP(init){ this._p=[]; this._cb=null;
    if(init instanceof USP){ this._p=init._p.map(function(x){return [x[0],x[1]];}); }
    else if(typeof init==='string'){ this._p=parseQuery(init); }
    else if(init && typeof init==='object'){ for(var k in init){ this._p.push([String(k),String(init[k])]); } } }
  USP.prototype._sync=function(){ if(this._cb) this._cb(this.toString()); };
  USP.prototype.append=function(k,v){ this._p.push([String(k),String(v)]); this._sync(); };
  USP.prototype.set=function(k,v){ k=String(k); var seen=false;
    this._p=this._p.filter(function(x){ if(x[0]===k){ if(!seen){seen=true; x[1]=String(v); return true;} return false; } return true; });
    if(!seen) this._p.push([k,String(v)]); this._sync(); };
  USP.prototype.get=function(k){ k=String(k); for(var i=0;i<this._p.length;i++) if(this._p[i][0]===k) return this._p[i][1]; return null; };
  USP.prototype.getAll=function(k){ k=String(k); return this._p.filter(function(x){return x[0]===k;}).map(function(x){return x[1];}); };
  USP.prototype.has=function(k){ return this.get(String(k))!==null; };
  USP.prototype['delete']=function(k){ k=String(k); this._p=this._p.filter(function(x){return x[0]!==k;}); this._sync(); };
  USP.prototype.forEach=function(cb,t){ this._p.forEach(function(x){ cb.call(t,x[1],x[0],this); },this); };
  USP.prototype.keys=function(){ return this._p.map(function(x){return x[0];})[Symbol.iterator](); };
  USP.prototype.values=function(){ return this._p.map(function(x){return x[1];})[Symbol.iterator](); };
  USP.prototype.entries=function(){ return this._p.map(function(x){return [x[0],x[1]];})[Symbol.iterator](); };
  USP.prototype[Symbol.iterator]=function(){ return this.entries(); };
  USP.prototype.toString=function(){ return this._p.map(function(x){return enc(x[0])+'='+enc(x[1]);}).join('&'); };
  Object.defineProperty(USP.prototype,'size',{get:function(){return this._p.length;}});

  function bindSP(url){ var s=url._sp; s._cb=function(qs){ url._set('search', qs?('?'+qs):''); }; return s; }
  function URL(input, base){ if(!(this instanceof URL)) throw new TypeError("Constructor URL requires 'new'");
    this._c=P(String(input), (base===undefined||base===null)?undefined:String(base));
    this._sp=new USP(this._c.search); bindSP(this); }
  URL.prototype._set=function(key,val){ this._c=P(W(this._c.href,key,String(val))); };
  function defGetSet(name){ Object.defineProperty(URL.prototype,name,{ enumerable:true,
    get:function(){ return this._c[name]; },
    set:function(v){ if(name==='href'){ this._c=P(String(v)); this._sp=new USP(this._c.search); bindSP(this); }
      else { this._set(name,v); if(name==='search'){ this._sp=new USP(this._c.search); bindSP(this); } } } }); }
  ['href','protocol','username','password','hostname','port','pathname','search','hash'].forEach(defGetSet);
  Object.defineProperty(URL.prototype,'host',{enumerable:true,get:function(){return this._c.host;}});
  Object.defineProperty(URL.prototype,'origin',{enumerable:true,get:function(){return this._c.origin;}});
  Object.defineProperty(URL.prototype,'searchParams',{enumerable:true,get:function(){return this._sp;}});
  URL.prototype.toString=function(){ return this._c.href; };
  URL.prototype.toJSON=function(){ return this._c.href; };
  URL.canParse=function(i,b){ try{ P(String(i),(b===undefined||b===null)?undefined:String(b)); return true; }catch(e){ return false; } };
  URL.parse=function(i,b){ try{ return new URL(i,b); }catch(e){ return null; } };
  g.URL=URL; g.URLSearchParams=USP;
})(typeof globalThis!=='undefined'?globalThis:this);
"#;
