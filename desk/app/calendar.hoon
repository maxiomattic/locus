::  %calendar — clan calendar store.
::
/-  cal=calendar
/+  default-agent, dbug, server
/+  calendar-json
/*  openapi-spec  %json  /app/calendar/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-0:cal
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
      [%x %events ~]
    ``json+!>((events:enjs:calendar-json events))
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
  (emit [%pass /eyre/calendar %arvo %e %connect [~ /calendar] %calendar])
::
++  load
  |=  =vase
  ^+  cor
  =/  tried=(each state-0:cal tang)
    (mule |.(!<(state-0:cal vase)))
  ?:  ?=(%| -.tried)  init
  =.  state  p.tried
  (emit [%pass /eyre/calendar %arvo %e %connect [~ /calendar] %calendar])
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
  ?:  &(=(site ~[%calendar %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  ?:  ?=([%calendar %~.~ %v1 %events ~] site)
    ?:  =(%'GET' method)
      (give-json eyre-id (ok:enjs:calendar-json [events+(events:enjs:calendar-json events)]~))
    ?:  =(%'POST' method)
      =^  out=json  cor  (body-json inbound-request 'event.create')
      (give-json eyre-id out)
    (http-error eyre-id 405 'method not allowed')
  ?:  ?=([%calendar %~.~ %v1 %events @ ~] site)
    ?~  id=(parse-id i.t.t.t.t.site)
      (http-error eyre-id 400 'bad id')
    ?:  =(%'GET' method)
      ?~  e=(~(get by events) u.id)
        (give-json eyre-id (err:enjs:calendar-json 'not-found' 'no such event'))
      (give-json eyre-id (ok:enjs:calendar-json [event+(event:enjs:calendar-json u.e)]~))
    ?:  =(%'PUT' method)
      =^  out=json  cor  (patch-id u.id inbound-request)
      (give-json eyre-id out)
    ?:  =(%'DELETE' method)
      (delete-event eyre-id u.id args.request-line)
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
  (give-http eyre-id (status-of:calendar-json json) 'application/json' (en:json:html json))
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
++  delete-event
  |=  [eyre-id=@ta id=@ud args=(list [key=@t value=@t])]
  ^+  cor
  ?~  old=(~(get by events) id)
    (give-json-status eyre-id (err:enjs:calendar-json 'not-found' 'no such event'))
  ?~  raw=(arg-value args 'expectedRevision')
    (remove-event eyre-id id)
  ?~  exp=(parse-id u.raw)
    (give-json-status eyre-id (err:enjs:calendar-json 'bad-request' 'expectedRevision must be a number'))
  ?.  =(u.exp revision.u.old)
    (give-json-status eyre-id (err:enjs:calendar-json 'conflict' 'stale expectedRevision'))
  (remove-event eyre-id id)
++  remove-event
  |=  [eyre-id=@ta id=@ud]
  ^+  cor
  =.  events  (~(del by events) id)
  (give-json-status eyre-id (ok:enjs:calendar-json [id+(numb:enjs:format id)]~))
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
    [(err:enjs:calendar-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:calendar-json 'bad-request' 'invalid json') cor]
  (dispatch fn u.jon)
::
++  patch-id
  |=  [id=@ud req=inbound-request:eyre]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:calendar-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:calendar-json 'bad-request' 'invalid json') cor]
  ?>  ?=(%o -.u.jon)
  =.  p.u.jon  (~(put by p.u.jon) 'id' (numb:enjs:format id))
  (dispatch 'event.patch' u.jon)
::
++  run-json
  |=  jon=json
  ^-  [json _cor]
  =/  parsed=(each [@t json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:calendar-json 'bad-request' 'malformed call') cor]
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
  ?:  =(fn 'event.create')  (create-event args)
  ?:  =(fn 'event.patch')   (patch-event args)
  [(err:enjs:calendar-json 'unknown-function' fn) cor]
::
++  unix-da
  |=  n=@ud
  ^-  @da
  (add ~1970.1.1 (mul n ~s1))
::
++  create-event
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [title=@t start=@da end=@da body=@t calendar=@t] tang)
    (mule |.((parse-create args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:calendar-json 'bad-request' 'need title, start, end') cor]
  =/  [title=@t start=@da end=@da body=@t calendar=@t]  p.parsed
  ?:  (gte start end)
    [(err:enjs:calendar-json 'bad-request' 'end must be after start') cor]
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  e=event:cal  [id calendar title start end body 0]
  =.  events  (~(put by events) id e)
  [(ok:enjs:calendar-json [event+(event:enjs:calendar-json e)]~) cor]
::
++  parse-create
  |=  args=json
  ^-  [title=@t start=@da end=@da body=@t calendar=@t]
  ?>  ?=(%o -.args)
  =/  title=@t  (so:dejs:format (~(got by p.args) 'title'))
  =/  start=@da  (unix-da (ni:dejs:format (~(got by p.args) 'start')))
  =/  end=@da  (unix-da (ni:dejs:format (~(got by p.args) 'end')))
  =/  body=@t
    ?~  b=(~(get by p.args) 'body')  ''
    (so:dejs:format u.b)
  =/  calendar=@t
    ?~  c=(~(get by p.args) 'calendar')  'personal'
    (so:dejs:format u.c)
  [title start end body calendar]
::
++  patch-event
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@ud @ud title=@t start=@da end=@da body=@t] tang)
    (mule |.((parse-patch args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:calendar-json 'bad-request' 'need id, expectedRevision') cor]
  =/  [id=@ud exp=@ud title=@t start=@da end=@da body=@t]  p.parsed
  ?~  old=(~(get by events) id)
    [(err:enjs:calendar-json 'not-found' 'no such event') cor]
  ?.  =(exp revision.u.old)
    [(err:enjs:calendar-json 'conflict' 'stale expectedRevision') cor]
  ?:  (gte start end)
    [(err:enjs:calendar-json 'bad-request' 'end must be after start') cor]
  =/  e=event:cal
    u.old(title title, start start, end end, body body, revision +(revision.u.old))
  =.  events  (~(put by events) id e)
  [(ok:enjs:calendar-json [event+(event:enjs:calendar-json e)]~) cor]
::
++  parse-patch
  |=  args=json
  ^-  [@ud @ud @t @da @da @t]
  ?>  ?=(%o -.args)
  =/  id=@ud  (ni:dejs:format (~(got by p.args) 'id'))
  =/  exp=@ud  (ni:dejs:format (~(got by p.args) 'expectedRevision'))
  ?~  old=(~(get by events) id)
    [id exp '' ~1970.1.1 ~1970.1.1 '']
  =/  title=@t
    ?~  t=(~(get by p.args) 'title')  title.u.old
    (so:dejs:format u.t)
  =/  start=@da
    ?~  s=(~(get by p.args) 'start')  start.u.old
    (unix-da (ni:dejs:format u.s))
  =/  end=@da
    ?~  e=(~(get by p.args) 'end')  end.u.old
    (unix-da (ni:dejs:format u.e))
  =/  body=@t
    ?~  b=(~(get by p.args) 'body')  body.u.old
    (so:dejs:format u.b)
  [id exp title start end body]
--
