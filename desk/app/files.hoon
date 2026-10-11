::  %files — catalog only. Bytes live on ZFS behind a host gateway, by SHA-256.
::
/-  fil=files
/+  default-agent, dbug, server
/+  files-json
/*  openapi-spec  %json  /app/files/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-0:fil
--
=|  current-state
=*  state  -
%-  agent:dbug
^-  agent:gall
=<
|_  =bowl:gall
+*  this  .
    def   ~(. (default-agent this %|) bowl)
    cor   ~(. +> [bowl ~])
::
++  on-init
  ^-  (quip card _this)
  =^  cards  state
    abet:init:cor
  [cards this]
::
++  on-save  !>(state)
++  on-load
  |=  old=vase
  ^-  (quip card _this)
  =^  cards  state
    abet:(load:cor old)
  [cards this]
::
++  on-poke
  |=  [=mark =vase]
  ^-  (quip card _this)
  =^  cards  state
    abet:(poke:cor mark vase)
  [cards this]
::
++  on-watch
  |=  =path
  ^-  (quip card _this)
  ?+  path  (on-watch:def path)
    [%http-response *]  `this
  ==
::
++  on-leave  on-leave:def
++  on-peek
  |=  =path
  ^-  (unit (unit cage))
  ?+  path  ~
      [%x %v0 %api-key ~]
    ``json+!>(?~(api-key ~ s+u.api-key))
      [%x %files ~]
    ``json+!>((files:enjs:files-json ~(val by files)))
  ==
::
++  on-agent  on-agent:def
++  on-arvo
  |=  [=wire =sign-arvo]
  ^-  (quip card _this)
  ?.  ?=([%eyre *] wire)  (on-arvo:def wire sign-arvo)
  `this
++  on-fail  on-fail:def
--
::
|_  [=bowl:gall cards=(list card)]
++  abet  [(flop cards) state]
++  cor   .
++  emit  |=(=card cor(cards [card cards]))
++  emil  |=(caz=(list card) cor(cards (welp (flop caz) cards)))
++  give  |=(=gift:agent:gall (emit %give gift))
::
++  init
  ^+  cor
  =.  api-key  `(scot %uv eny.bowl)
  =.  next-id  1
  (emit [%pass /eyre/files %arvo %e %connect [~ /files] %files])
::
++  load
  |=  =vase
  ^+  cor
  =/  tried=(each state-0:fil tang)
    (mule |.(!<(state-0:fil vase)))
  ?:  ?=(%| -.tried)  init
  =.  state  p.tried
  (emit [%pass /eyre/files %arvo %e %connect [~ /files] %files])
::
++  poke
  |=  [=mark =vase]
  ^+  cor
  ?+  mark  ~|(bad-mark+mark !!)
      %handle-http-request
    (serve-http !<([eyre-id=@ta =inbound-request:eyre] vase))
      %json
    ?>  =(our.bowl src.bowl)
    =^  jon  cor  (run-json !<(json vase))
    cor
  ==
::
++  serve-http
  |=  [eyre-id=@ta =inbound-request:eyre]
  ^+  cor
  =/  =request-line:server
    (parse-request-line:server url.request.inbound-request)
  =*  site  site.request-line
  =/  method=@tas  method.request.inbound-request
  ?:  &(=(site ~[%files %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  ?:  ?=([%files %~.~ %v1 %files ~] site)
    ?:  =(%'GET' method)
      =/  dir=(unit @t)  (arg-value args.request-line 'dir')
      =/  listed=(list file:fil)
        ?~  dir  ~(val by files)
        (skim ~(val by files) |=(f=file:fil =(dir.f u.dir)))
      (give-json-status eyre-id (ok:enjs:files-json [files+(files:enjs:files-json listed)]~))
    ?:  =(%'POST' method)
      =^  out=json  cor  (body-json inbound-request 'file.create')
      (give-json-status eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  ?:  ?=([%files %~.~ %v1 %files @ ~] site)
    ?~  id=(parse-id i.t.t.t.t.site)
      (give-json-status eyre-id (err:enjs:files-json 'bad-request' 'bad id'))
    ?:  =(%'GET' method)
      ?~  f=(~(get by files) u.id)
        (give-json-status eyre-id (err:enjs:files-json 'not-found' 'no such file'))
      (give-json-status eyre-id (ok:enjs:files-json [file+(file:enjs:files-json u.f)]~))
    ?:  =(%'PUT' method)
      =^  out=json  cor  (patch-id u.id inbound-request)
      (give-json-status eyre-id out)
    ?:  =(%'DELETE' method)
      =^  out=json  cor  (delete-id u.id args.request-line)
      (give-json-status eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  ?:  ?=([%files %~.~ %v1 %dirs ~] site)
    ?:  =(%'GET' method)
      (give-json-status eyre-id (ok:enjs:files-json [dirs+(dirs:enjs:files-json all-dirs)]~))
    ?:  =(%'POST' method)
      =^  out=json  cor  (body-json inbound-request 'dir.create')
      (give-json-status eyre-id out)
    ?:  =(%'DELETE' method)
      ?~  p=(arg-value args.request-line 'path')
        (give-json-status eyre-id (err:enjs:files-json 'bad-request' 'need path query'))
      =^  out=json  cor  (dispatch 'dir.delete' (pairs:enjs:format ~[path+s+u.p]))
      (give-json-status eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  (http-error eyre-id 404 'not found')
::
++  give-http
  |=  [eyre-id=@ta code=@ud ct=@t body=@t]
  ^+  cor
  =/  data=octs  (as-octs:mimes:html body)
  %-  emil
  :~  [%give %fact [/http-response/[eyre-id]]~ %http-response-header !>(`response-header:http`[code ~[['content-type' ct]]])]
      [%give %fact [/http-response/[eyre-id]]~ %http-response-data !>(`data)]
      [%give %kick [/http-response/[eyre-id]]~ ~]
  ==
::
++  http-error
  |=  [eyre-id=@ta code=@ud message=@t]
  (give-http eyre-id code 'text/plain' message)
::
++  give-json-status
  |=  [eyre-id=@ta =json]
  ^+  cor
  (give-http eyre-id (status-of:files-json json) 'application/json' (en:json:html json))
::
++  get-api-key-header
  |=  req=inbound-request:eyre
  ^-  (unit @t)
  =/  hdrs=(list [key=@t value=@t])  header-list.request.req
  |-  ^-  (unit @t)
  ?~  hdrs  ~
  ?:  =((cass (trip key.i.hdrs)) "x-api-key")  `value.i.hdrs
  $(hdrs t.hdrs)
::
++  authorized
  |=  req=inbound-request:eyre
  ^-  ?
  ?:  authenticated.req  &
  ?~  api-key  |
  ?~  hdr=(get-api-key-header req)  |
  =(u.hdr u.api-key)
::
++  arg-value
  |=  [args=(list [key=@t value=@t]) k=@t]
  ^-  (unit @t)
  |-
  ?~  args  ~
  ?:  =(key.i.args k)  `value.i.args
  $(args t.args)
::
::  decimal id. slaw %ud wants dots from 1.000 up, so insert one before the
::  last three digits (ids above 999,999 are rejected).
++  parse-id
  |=  t=@t
  ^-  (unit @ud)
  =/  s=tape  (trip t)
  ?:  (lte (lent s) 3)  (slaw %ud t)
  ?:  (gth (lent s) 6)  ~
  =/  cut=@ud  (sub (lent s) 3)
  (slaw %ud (crip :(weld (scag cut s) "." (slag cut s))))
::
++  bad
  |=  msg=@t
  ^-  [json _cor]
  [(err:enjs:files-json 'bad-request' msg) cor]
::
++  body-json
  |=  [req=inbound-request:eyre fn=@t]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:files-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:files-json 'bad-request' 'invalid json') cor]
  (dispatch fn u.jon)
::
++  patch-id
  |=  [id=@ud req=inbound-request:eyre]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:files-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:files-json 'bad-request' 'invalid json') cor]
  ?>  ?=(%o -.u.jon)
  =.  p.u.jon  (~(put by p.u.jon) 'id' (numb:enjs:format id))
  (dispatch 'file.patch' u.jon)
::
++  delete-id
  |=  [id=@ud args=(list [key=@t value=@t])]
  ^-  [json _cor]
  ?~  rev=(arg-value args 'expectedRevision')
    (dispatch 'file.delete' (pairs:enjs:format ~[id+(numb:enjs:format id)]))
  ?~  n=(parse-id u.rev)
    (bad 'expectedRevision must be a number')
  (dispatch 'file.delete' (pairs:enjs:format ~[id+(numb:enjs:format id) 'expectedRevision'^(numb:enjs:format u.n)]))
::
++  run-json
  |=  jon=json
  ^-  [json _cor]
  =/  parsed=(each [@t json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:files-json 'bad-request' 'malformed call') cor]
  (dispatch p.parsed)
::
++  parse-call
  |=  jon=json
  ^-  [@t json]
  ?>  ?=(%o -.jon)
  :-  (so:dejs:format (~(got by p.jon) 'fn'))
  (fall (~(get by p.jon) 'args') ~)
::
++  dispatch
  |=  [fn=@t args=json]
  ^-  [json _cor]
  ?:  =(fn 'file.create')  (create-file args)
  ?:  =(fn 'file.patch')   (patch-file args)
  ?:  =(fn 'file.delete')  (delete-file args)
  ?:  =(fn 'dir.create')   (create-dir args)
  ?:  =(fn 'dir.delete')   (delete-dir args)
  [(err:enjs:files-json 'unknown-function' fn) cor]
::
::  validation
::
++  split-path
  |=  t=tape
  ^-  (list tape)
  =|  cur=tape
  =|  out=(list tape)
  |-  ^-  (list tape)
  ?~  t  (flop [(flop cur) out])
  ?:  =('/' i.t)  $(t t.t, cur ~, out [(flop cur) out])
  $(t t.t, cur [i.t cur])
::
::  "/" or an absolute path with no empty segments and no ".." segments
++  valid-dir
  |=  d=@t
  ^-  ?
  =/  t=tape  (trip d)
  ?~  t  %.n
  ?.  =('/' i.t)  %.n
  ?:  =("/" t)  %.y
  =/  segs=(list tape)  (slag 1 (split-path t))
  ?:  (lien segs |=(s=tape =(s ~)))  %.n
  !(lien segs |=(s=tape =(s "..")))
::
++  valid-name
  |=  n=@t
  ^-  ?
  =/  t=tape  (trip n)
  ?&  ?=(^ t)
      ?=(~ (find "/" t))
  ==
::
++  valid-sha
  |=  h=@t
  ^-  ?
  =/  t=tape  (trip h)
  ?.  =(64 (lent t))  %.n
  %+  levy  t
  |=  c=@tD
  ?|  &((gte c '0') (lte c '9'))
      &((gte c 'a') (lte c 'f'))
  ==
::
::  "/a/b" -> ~["/" "/a" "/a/b"]
++  ancestors
  |=  d=@t
  ^-  (list @t)
  =/  segs=(list tape)  (slag 1 (split-path (trip d)))
  =|  out=(list @t)
  =|  pre=tape
  |-  ^-  (list @t)
  ?~  segs  ['/' out]
  =.  pre  :(weld pre "/" i.segs)
  $(segs t.segs, out [(crip pre) out])
::
::  true when d is strictly below dpath
++  under
  |=  [dpath=@t d=@t]
  ^-  ?
  =/  pre=tape  (weld (trip dpath) "/")
  =(pre (scag (lent pre) (trip d)))
::
++  has-entries
  |=  dpath=@t
  ^-  ?
  %+  lien  ~(val by files)
  |=(f=file:fil |(=(dir.f dpath) (under dpath dir.f)))
::
++  all-dirs
  ^-  (list @t)
  =/  leaves=(list @t)
    (weld (turn ~(val by files) |=(f=file:fil dir.f)) ~(tap in dirs))
  =/  out=(set @t)  (silt ['/' (zing (turn leaves ancestors))])
  (sort ~(tap in out) aor)
::
::  files
::
++  parse-create
  |=  args=json
  ^-  [dir=@t name=@t sha=@t size=@ud mime=@t]
  ?>  ?=(%o -.args)
  =/  mime=@t
    ?~  m=(~(get by p.args) 'mime')  'application/octet-stream'
    (so:dejs:format u.m)
  :*  (so:dejs:format (~(got by p.args) 'dir'))
      (so:dejs:format (~(got by p.args) 'name'))
      (so:dejs:format (~(got by p.args) 'sha256'))
      (ni:dejs:format (~(got by p.args) 'size'))
      mime
  ==
::
++  create-file
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [dir=@t name=@t sha=@t size=@ud mime=@t] tang)
    (mule |.((parse-create args)))
  ?:  ?=(%| -.parsed)
    (bad 'need dir, name, sha256, size')
  =/  [dir=@t name=@t sha=@t size=@ud mime=@t]  p.parsed
  ?.  (valid-dir dir)  (bad 'dir must be absolute, with no empty or .. segments')
  ?.  (valid-name name)  (bad 'name must be non-empty and contain no /')
  ?.  (valid-sha sha)  (bad 'sha256 must be 64 lowercase hex characters')
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  f=file:fil  [id dir name sha size mime now.bowl now.bowl 0]
  =.  files  (~(put by files) id f)
  [(ok:enjs:files-json [file+(file:enjs:files-json f)]~) cor]
::
++  parse-patch
  |=  args=json
  ^-  [@ud @ud (unit @t) (unit @t)]
  ?>  ?=(%o -.args)
  :*  (ni:dejs:format (~(got by p.args) 'id'))
      (ni:dejs:format (~(got by p.args) 'expectedRevision'))
      (opt-str args 'dir')
      (opt-str args 'name')
  ==
::
++  opt-str
  |=  [args=json key=@t]
  ^-  (unit @t)
  ?>  ?=(%o -.args)
  ?~  v=(~(get by p.args) key)  ~
  `(so:dejs:format u.v)
::
++  patch-file
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@ud @ud (unit @t) (unit @t)] tang)
    (mule |.((parse-patch args)))
  ?:  ?=(%| -.parsed)
    (bad 'need id, expectedRevision')
  =/  [id=@ud exp=@ud dir=(unit @t) name=(unit @t)]  p.parsed
  ?~  old=(~(get by files) id)
    [(err:enjs:files-json 'not-found' 'no such file') cor]
  ?.  =(exp revision.u.old)
    [(err:enjs:files-json 'conflict' 'stale expectedRevision') cor]
  =/  new-dir=@t  ?~(dir dir.u.old u.dir)
  =/  new-name=@t  ?~(name name.u.old u.name)
  ?.  (valid-dir new-dir)  (bad 'dir must be absolute, with no empty or .. segments')
  ?.  (valid-name new-name)  (bad 'name must be non-empty and contain no /')
  =/  f=file:fil
    u.old(dir new-dir, name new-name, modified now.bowl, revision +(revision.u.old))
  =.  files  (~(put by files) id f)
  [(ok:enjs:files-json [file+(file:enjs:files-json f)]~) cor]
::
++  parse-delete
  |=  args=json
  ^-  [@ud (unit @ud)]
  ?>  ?=(%o -.args)
  :-  (ni:dejs:format (~(got by p.args) 'id'))
  ?~  e=(~(get by p.args) 'expectedRevision')  ~
  `(ni:dejs:format u.e)
::
++  delete-file
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@ud (unit @ud)] tang)
    (mule |.((parse-delete args)))
  ?:  ?=(%| -.parsed)
    (bad 'need id')
  =/  [id=@ud exp=(unit @ud)]  p.parsed
  ?~  old=(~(get by files) id)
    [(err:enjs:files-json 'not-found' 'no such file') cor]
  =/  stale=?  ?~(exp %.n !=(u.exp revision.u.old))
  ?:  stale
    [(err:enjs:files-json 'conflict' 'stale expectedRevision') cor]
  =.  files  (~(del by files) id)
  [(ok:enjs:files-json [id+(numb:enjs:format id)]~) cor]
::
::  folders
::
++  path-arg
  |=  args=json
  ^-  @t
  ?>  ?=(%o -.args)
  (so:dejs:format (~(got by p.args) 'path'))
::
++  create-dir
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each @t tang)  (mule |.((path-arg args)))
  ?:  ?=(%| -.parsed)
    (bad 'need path')
  =/  dpath=@t  p.parsed
  ?.  (valid-dir dpath)  (bad 'path must be absolute, with no empty or .. segments')
  =.  dirs  (~(put in dirs) dpath)
  [(ok:enjs:files-json [path+s+dpath]~) cor]
::
++  delete-dir
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each @t tang)  (mule |.((path-arg args)))
  ?:  ?=(%| -.parsed)
    (bad 'need path')
  =/  dpath=@t  p.parsed
  ?.  (valid-dir dpath)  (bad 'path must be absolute, with no empty or .. segments')
  ?:  =('/' dpath)  (bad 'cannot delete the root folder')
  ?:  (has-entries dpath)
    [(err:enjs:files-json 'conflict' 'folder is not empty') cor]
  =.  dirs  (silt (skip ~(tap in dirs) |=(d=@t |(=(d dpath) (under dpath d)))))
  [(ok:enjs:files-json [path+s+dpath]~) cor]
--
