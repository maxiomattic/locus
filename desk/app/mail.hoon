::  %mail — household mail store (P2).
::
/-  mail
/+  default-agent, dbug, server
/+  mail-json
/*  openapi-spec  %json  /app/mail/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-0:mail
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
      [%x %messages ~]
    ``json+!>((messages:enjs:mail-json messages))
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
  (emit [%pass /eyre/mail %arvo %e %connect [~ /mail] %mail])
::
++  load
  |=  =vase
  ^+  cor
  =/  tried=(each state-0:mail tang)
    (mule |.(!<(state-0:mail vase)))
  ?:  ?=(%| -.tried)  init
  =.  state  p.tried
  (emit [%pass /eyre/mail %arvo %e %connect [~ /mail] %mail])
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
  ?:  &(=(site ~[%mail %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  ?:  ?=([%mail %~.~ %v1 %ingest ~] site)
    ?.  =(%'POST' method)  (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (body-json inbound-request 'ingest')
    (give-json eyre-id out)
  ?:  ?=([%mail %~.~ %v1 %messages ~] site)
    ?:  =(%'GET' method)
      (give-json eyre-id (ok:enjs:mail-json [messages+(list-folder args.request-line)]~))
    ?:  =(%'POST' method)
      =^  out=json  cor  (body-json inbound-request 'message.create')
      (give-json eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  ?:  ?=([%mail %~.~ %v1 %messages @ %send ~] site)
    ?.  =(%'POST' method)  (http-error eyre-id 405 'method not allowed')
    ?~  id=(slaw %ud i.t.t.t.t.site)
      (http-error eyre-id 400 'bad id')
    =^  out=json  cor  (send-id u.id)
    (give-json eyre-id out)
  ?:  ?=([%mail %~.~ %v1 %messages @ ~] site)
    ?~  id=(slaw %ud i.t.t.t.t.site)
      (http-error eyre-id 400 'bad id')
    ?:  =(%'GET' method)
      ?~  m=(~(get by messages) u.id)
        (give-json eyre-id (err:enjs:mail-json 'not-found' 'no such message'))
      (give-json eyre-id (ok:enjs:mail-json [msg+(message:enjs:mail-json u.m)]~))
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
    [(err:enjs:mail-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:mail-json 'bad-request' 'invalid json') cor]
  (dispatch fn u.jon)
::
++  patch-id
  |=  [id=@ud req=inbound-request:eyre]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:mail-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:mail-json 'bad-request' 'invalid json') cor]
  ?>  ?=(%o -.u.jon)
  =.  p.u.jon  (~(put by p.u.jon) 'id' (numb:enjs:format id))
  (dispatch 'message.patch' u.jon)
::
++  run-json
  |=  jon=json
  ^-  [json _cor]
  =/  parsed=(each [@t json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:mail-json 'bad-request' 'malformed call') cor]
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
  ?:  =(fn 'message.create')  (create-draft args)
  ?:  =(fn 'message.patch')   (patch-msg args)
  ?:  =(fn 'ingest')          (ingest args)
  ?:  =(fn 'send')            (send-args args)
  [(err:enjs:mail-json 'unknown-function' fn) cor]
::
++  list-folder
  |=  args=(list [@t @t])
  ^-  json
  =/  want=(unit folder:mail)
    ?~  a=(get-arg args 'folder')  ~
    `(mule-folder u.a)
  =/  all=(list message:mail)  ~(val by messages)
  =/  keep=(list message:mail)
    ?~  want  all
    (skim all |=(m=message:mail =(folder.m u.want)))
  a+(turn keep message:enjs:mail-json)
::
++  get-arg
  |=  [args=(list [@t @t]) key=@t]
  ^-  (unit @t)
  |-
  ?~  args  ~
  ?:  =(-.i.args key)  `+.i.args
  $(args t.args)
::
++  mule-folder
  |=  t=@t
  ^-  folder:mail
  ?:  =(t 'inbox')   %inbox
  ?:  =(t 'sent')    %sent
  ?:  =(t 'drafts')  %drafts
  ?:  =(t 'trash')   %trash
  %inbox
::
++  parse-to
  |=  args=json
  ^-  (list @t)
  ?>  ?=(%o -.args)
  ?~  j=(~(get by p.args) 'to')  ~
  ?+  -.u.j  ~
    %s  [p.u.j ~]
    %a  (turn p.u.j |=(x=json (so:dejs:format x)))
  ==
::
++  cord-or
  |=  [args=json key=@t fallback=@t]
  ^-  @t
  ?>  ?=(%o -.args)
  ?~  j=(~(get by p.args) key)  fallback
  (so:dejs:format u.j)
::
++  create-draft
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [from=@t to=(list @t) subject=@t body=@t] tang)
    (mule |.([(cord-or args 'from' (scot %p our.bowl)) (parse-to args) (cord-or args 'subject' '') (cord-or args 'body' '')]))
  ?:  ?=(%| -.parsed)
    [(err:enjs:mail-json 'bad-request' 'need subject') cor]
  =/  [from=@t to=(list @t) subject=@t body=@t]  p.parsed
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  m=message:mail  [id %drafts from to subject body now.bowl 0]
  =.  messages  (~(put by messages) id m)
  [(ok:enjs:mail-json [msg+(message:enjs:mail-json m)]~) cor]
::
++  ingest
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [from=@t to=(list @t) subject=@t body=@t] tang)
    (mule |.([(cord-or args 'from' '') (parse-to args) (cord-or args 'subject' '') (cord-or args 'body' '')]))
  ?:  ?=(%| -.parsed)
    [(err:enjs:mail-json 'bad-request' 'need from') cor]
  =/  [from=@t to=(list @t) subject=@t body=@t]  p.parsed
  ?:  =('' from)
    [(err:enjs:mail-json 'bad-request' 'need from') cor]
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  m=message:mail  [id %inbox from to subject body now.bowl 0]
  =.  messages  (~(put by messages) id m)
  [(ok:enjs:mail-json [msg+(message:enjs:mail-json m)]~) cor]
::
++  patch-msg
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@ud @ud @t @t] tang)
    (mule |.((parse-patch args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:mail-json 'bad-request' 'need id, expectedRevision') cor]
  =/  [id=@ud exp=@ud subject=@t body=@t]  p.parsed
  ?~  old=(~(get by messages) id)
    [(err:enjs:mail-json 'not-found' 'no such message') cor]
  ?.  =(exp revision.u.old)
    [(err:enjs:mail-json 'conflict' 'stale expectedRevision') cor]
  =/  m=message:mail
    u.old(subject subject, body body, revision +(revision.u.old), at now.bowl)
  =.  messages  (~(put by messages) id m)
  [(ok:enjs:mail-json [msg+(message:enjs:mail-json m)]~) cor]
::
++  parse-patch
  |=  args=json
  ^-  [@ud @ud @t @t]
  ?>  ?=(%o -.args)
  =/  id=@ud  (ni:dejs:format (~(got by p.args) 'id'))
  =/  exp=@ud  (ni:dejs:format (~(got by p.args) 'expectedRevision'))
  ?~  old=(~(get by messages) id)
    [id exp '' '']
  :^  id  exp  (cord-or args 'subject' subject.u.old)
    (cord-or args 'body' body.u.old)
::
++  send-args
  |=  args=json
  ^-  [json _cor]
  ?.  ?=(%o -.args)
    [(err:enjs:mail-json 'bad-request' 'need id') cor]
  =/  parsed=(each @ud tang)
    (mule |.((ni:dejs:format (~(got by p.args) 'id'))))
  ?:  ?=(%| -.parsed)
    [(err:enjs:mail-json 'bad-request' 'need id') cor]
  (send-id p.parsed)
::
++  send-id
  |=  id=@ud
  ^-  [json _cor]
  ?~  old=(~(get by messages) id)
    [(err:enjs:mail-json 'not-found' 'no such message') cor]
  ?.  =(%drafts folder.u.old)
    [(err:enjs:mail-json 'conflict' 'only drafts can be sent') cor]
  =/  m=message:mail  u.old(folder %sent, at now.bowl)
  =.  messages  (~(put by messages) id m)
  [(ok:enjs:mail-json [msg+(message:enjs:mail-json m)]~) cor]
--
