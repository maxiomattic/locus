::  %contacts — clan people.
::
/-  con=contacts
/+  default-agent, dbug, server
/+  contacts-json
/*  openapi-spec  %json  /app/contacts/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-0:con
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
      [%x %contacts ~]
    ``json+!>((contacts:enjs:contacts-json contacts))
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
  (emit [%pass /eyre/contacts %arvo %e %connect [~ /contacts] %contacts])
::
++  load
  |=  =vase
  ^+  cor
  ::  Pill leftover %contacts is Tlon's agent (negotiate %2). Our state is
  ::  %0; a nest-fail here must reset, not crash mill of the rest of desk.bill.
  =/  tried=(each state-0:con tang)
    (mule |.(!<(state-0:con vase)))
  ?:  ?=(%| -.tried)  init
  =.  state  p.tried
  (emit [%pass /eyre/contacts %arvo %e %connect [~ /contacts] %contacts])
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
  ?:  &(=(site ~[%contacts %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  ?:  ?=([%contacts %~.~ %v1 %contacts ~] site)
    ?:  =(%'GET' method)
      (give-json eyre-id (ok:enjs:contacts-json [contacts+(contacts:enjs:contacts-json contacts)]~))
    ?:  =(%'POST' method)
      =^  out=json  cor  (body-json inbound-request 'contact.create')
      (give-json eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  ?:  ?=([%contacts %~.~ %v1 %contacts @ ~] site)
    ?~  id=(parse-id i.t.t.t.t.site)
      (http-error eyre-id 400 'bad id')
    ?:  =(%'GET' method)
      ?~  c=(~(get by contacts) u.id)
        (give-json eyre-id (err:enjs:contacts-json 'not-found' 'no such contact'))
      (give-json eyre-id (ok:enjs:contacts-json [contact+(contact:enjs:contacts-json u.c)]~))
    ?:  =(%'PUT' method)
      =^  out=json  cor  (patch-id u.id inbound-request)
      (give-json eyre-id out)
    ?:  =(%'DELETE' method)
      (delete-contact eyre-id u.id args.request-line)
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
++  give-json-status
  |=  [eyre-id=@ta =json]
  (give-http eyre-id (status-of:contacts-json json) 'application/json' (en:json:html json))
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
++  arg-value
  |=  [args=(list [key=@t value=@t]) k=@t]
  ^-  (unit @t)
  |-
  ?~  args  ~
  ?:  =(key.i.args k)  `value.i.args
  $(args t.args)
::
++  delete-contact
  |=  [eyre-id=@ta id=@ud args=(list [key=@t value=@t])]
  ^+  cor
  ?~  old=(~(get by contacts) id)
    (give-json-status eyre-id (err:enjs:contacts-json 'not-found' 'no such contact'))
  ?~  raw=(arg-value args 'expectedRevision')
    (remove-contact eyre-id id)
  ?~  exp=(parse-id u.raw)
    (give-json-status eyre-id (err:enjs:contacts-json 'bad-request' 'expectedRevision must be a number'))
  ?.  =(u.exp revision.u.old)
    (give-json-status eyre-id (err:enjs:contacts-json 'conflict' 'stale expectedRevision'))
  (remove-contact eyre-id id)
++  remove-contact
  |=  [eyre-id=@ta id=@ud]
  ^+  cor
  =.  contacts  (~(del by contacts) id)
  (give-json-status eyre-id (ok:enjs:contacts-json [id+(numb:enjs:format id)]~))
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
    [(err:enjs:contacts-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:contacts-json 'bad-request' 'invalid json') cor]
  (dispatch fn u.jon)
::
++  patch-id
  |=  [id=@ud req=inbound-request:eyre]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:contacts-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:contacts-json 'bad-request' 'invalid json') cor]
  ?>  ?=(%o -.u.jon)
  =.  p.u.jon  (~(put by p.u.jon) 'id' (numb:enjs:format id))
  (dispatch 'contact.patch' u.jon)
::
++  run-json
  |=  jon=json
  ^-  [json _cor]
  =/  parsed=(each [@t json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:contacts-json 'bad-request' 'malformed call') cor]
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
  ?:  =(fn 'contact.create')  (create-contact args)
  ?:  =(fn 'contact.patch')   (patch-contact args)
  [(err:enjs:contacts-json 'unknown-function' fn) cor]
::
++  create-contact
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [name=@t email=@t ship=@t] tang)
    (mule |.((parse-create args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:contacts-json 'bad-request' 'need name') cor]
  =/  [name=@t email=@t ship=@t]  p.parsed
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  c=contact:con  [id name email ship 0]
  =.  contacts  (~(put by contacts) id c)
  [(ok:enjs:contacts-json [contact+(contact:enjs:contacts-json c)]~) cor]
::
++  parse-create
  |=  args=json
  ^-  [name=@t email=@t ship=@t]
  ?>  ?=(%o -.args)
  :+  (so:dejs:format (~(got by p.args) 'name'))
    ?~  e=(~(get by p.args) 'email')  ''
    (so:dejs:format u.e)
  ?~  s=(~(get by p.args) 'ship')  ''
  (so:dejs:format u.s)
::
++  patch-contact
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@ud @ud name=@t email=@t ship=@t] tang)
    (mule |.((parse-patch args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:contacts-json 'bad-request' 'need id, expectedRevision') cor]
  =/  [id=@ud exp=@ud name=@t email=@t ship=@t]  p.parsed
  ?~  old=(~(get by contacts) id)
    [(err:enjs:contacts-json 'not-found' 'no such contact') cor]
  ?.  =(exp revision.u.old)
    [(err:enjs:contacts-json 'conflict' 'stale expectedRevision') cor]
  =/  c=contact:con
    u.old(name name, email email, ship ship, revision +(revision.u.old))
  =.  contacts  (~(put by contacts) id c)
  [(ok:enjs:contacts-json [contact+(contact:enjs:contacts-json c)]~) cor]
::
++  parse-patch
  |=  args=json
  ^-  [@ud @ud @t @t @t]
  ?>  ?=(%o -.args)
  =/  id=@ud  (ni:dejs:format (~(got by p.args) 'id'))
  =/  exp=@ud  (ni:dejs:format (~(got by p.args) 'expectedRevision'))
  ?~  old=(~(get by contacts) id)
    [id exp '' '' '']
  =/  name=@t
    ?~  n=(~(get by p.args) 'name')  name.u.old
    (so:dejs:format u.n)
  =/  email=@t
    ?~  e=(~(get by p.args) 'email')  email.u.old
    (so:dejs:format u.e)
  =/  ship=@t
    ?~  s=(~(get by p.args) 'ship')  ship.u.old
    (so:dejs:format u.s)
  [id exp name email ship]
--
