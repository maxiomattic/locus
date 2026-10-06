::  %photos — catalog only (P4). Bytes stay on ZFS.
::
/-  pho=photos
/+  default-agent, dbug, server
/+  photos-json
/*  openapi-spec  %json  /app/photos/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-0:pho
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
      [%x %photos ~]
    ``json+!>((photos:enjs:photos-json photos))
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
  (emit [%pass /eyre/photos %arvo %e %connect [~ /photos] %photos])
::
++  load
  |=  =vase
  ^+  cor
  =/  tried=(each state-0:pho tang)
    (mule |.(!<(state-0:pho vase)))
  ?:  ?=(%| -.tried)  init
  =.  state  p.tried
  (emit [%pass /eyre/photos %arvo %e %connect [~ /photos] %photos])
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
  ?:  &(=(site ~[%photos %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  ?:  ?=([%photos %~.~ %v1 %photos ~] site)
    ?:  =(%'GET' method)
      (give-json eyre-id (ok:enjs:photos-json [photos+(photos:enjs:photos-json photos)]~))
    ?:  =(%'POST' method)
      =^  out=json  cor  (body-json inbound-request 'photo.create')
      (give-json eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  ?:  ?=([%photos %~.~ %v1 %photos @ ~] site)
    ?~  id=(slaw %ud i.t.t.t.t.site)
      (http-error eyre-id 400 'bad id')
    ?:  =(%'GET' method)
      ?~  p=(~(get by photos) u.id)
        (give-json eyre-id (err:enjs:photos-json 'not-found' 'no such photo'))
      (give-json eyre-id (ok:enjs:photos-json [photo+(photo:enjs:photos-json u.p)]~))
    ?:  =(%'PUT' method)
      =^  out=json  cor  (patch-id u.id inbound-request)
      (give-json eyre-id out)
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
++  give-json
  |=  [eyre-id=@ta =json]
  (give-http eyre-id 200 'application/json' (en:json:html json))
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
++  body-json
  |=  [req=inbound-request:eyre fn=@t]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:photos-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:photos-json 'bad-request' 'invalid json') cor]
  (dispatch fn u.jon)
::
++  patch-id
  |=  [id=@ud req=inbound-request:eyre]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:photos-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:photos-json 'bad-request' 'invalid json') cor]
  ?>  ?=(%o -.u.jon)
  =.  p.u.jon  (~(put by p.u.jon) 'id' (numb:enjs:format id))
  (dispatch 'photo.patch' u.jon)
::
++  run-json
  |=  jon=json
  ^-  [json _cor]
  =/  parsed=(each [@t json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:photos-json 'bad-request' 'malformed call') cor]
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
  ?:  =(fn 'photo.create')  (create-photo args)
  ?:  =(fn 'photo.patch')   (patch-photo args)
  [(err:enjs:photos-json 'unknown-function' fn) cor]
::
++  safe-path
  |=  p=@t
  ^-  ?
  =/  t=tape  (trip p)
  ?&  ?=(^ t)
      !=('/' i.t)
      ?=(~ (find ".." t))
  ==
::
++  parse-tags
  |=  args=json
  ^-  (list @t)
  ?>  ?=(%o -.args)
  ?~  t=(~(get by p.args) 'tags')  ~
  ?>  ?=(%a -.u.t)
  (turn p.u.t so:dejs:format)
::
++  create-photo
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [album=@t title=@t path=@t tags=(list @t)] tang)
    (mule |.((parse-create args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:photos-json 'bad-request' 'need album, title, path') cor]
  =/  [album=@t title=@t path=@t tags=(list @t)]  p.parsed
  ?.  (safe-path path)
    [(err:enjs:photos-json 'bad-request' 'path must be relative with no ..') cor]
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  p=photo:pho  [id album title path tags 0]
  =.  photos  (~(put by photos) id p)
  [(ok:enjs:photos-json [photo+(photo:enjs:photos-json p)]~) cor]
::
++  parse-create
  |=  args=json
  ^-  [album=@t title=@t path=@t tags=(list @t)]
  ?>  ?=(%o -.args)
  :^    (so:dejs:format (~(got by p.args) 'album'))
      (so:dejs:format (~(got by p.args) 'title'))
    (so:dejs:format (~(got by p.args) 'path'))
  (parse-tags args)
::
++  patch-photo
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@ud @ud album=@t title=@t path=@t tags=(list @t)] tang)
    (mule |.((parse-patch args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:photos-json 'bad-request' 'need id, expectedRevision') cor]
  =/  [id=@ud exp=@ud album=@t title=@t path=@t tags=(list @t)]  p.parsed
  ?~  old=(~(get by photos) id)
    [(err:enjs:photos-json 'not-found' 'no such photo') cor]
  ?.  =(exp revision.u.old)
    [(err:enjs:photos-json 'conflict' 'stale expectedRevision') cor]
  ?.  (safe-path path)
    [(err:enjs:photos-json 'bad-request' 'path must be relative with no ..') cor]
  =/  p=photo:pho
    u.old(album album, title title, path path, tags tags, revision +(revision.u.old))
  =.  photos  (~(put by photos) id p)
  [(ok:enjs:photos-json [photo+(photo:enjs:photos-json p)]~) cor]
::
++  parse-patch
  |=  args=json
  ^-  [@ud @ud @t @t @t (list @t)]
  ?>  ?=(%o -.args)
  =/  id=@ud  (ni:dejs:format (~(got by p.args) 'id'))
  =/  exp=@ud  (ni:dejs:format (~(got by p.args) 'expectedRevision'))
  ?~  old=(~(get by photos) id)
    [id exp '' '' '' ~]
  =/  album=@t
    ?~  a=(~(get by p.args) 'album')  album.u.old
    (so:dejs:format u.a)
  =/  title=@t
    ?~  t=(~(get by p.args) 'title')  title.u.old
    (so:dejs:format u.t)
  =/  path=@t
    ?~  p=(~(get by p.args) 'path')  path.u.old
    (so:dejs:format u.p)
  =/  tags=(list @t)
    ?~  g=(~(get by p.args) 'tags')  tags.u.old
    ?>  ?=(%a -.u.g)
    (turn p.u.g so:dejs:format)
  [id exp album title path tags]
--
