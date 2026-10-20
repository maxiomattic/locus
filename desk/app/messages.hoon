::  %messages — direct messages and group conversations between ships. Each ship
::  keeps its own copy. Sending pokes the peer's %messages agent; its ack moves the
::  message from %pending to %delivered or %failed. A group message is poked to
::  every other member and is %delivered once all of them have acked.
::  Incoming pokes are stored as %received.
::
/-  msg=messages
/+  default-agent, dbug, server
/+  messages-json
/*  openapi-spec  %json  /app/messages/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-2:msg
::  longest message body, in bytes, sent or accepted
++  max-body  8.192
::  longest group title, in bytes
++  max-title  120
::  most members in one group, the owner included
++  max-members  50
::  most messages accepted from one peer within rate-window
++  rate-max  20
::  the rolling window the per-peer rate limit counts over
++  rate-window  ~s60
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
    ``json+!>((conversations:enjs:messages-json summaries:cor))
      [%x %groups ~]
    ``json+!>((groups:enjs:messages-json group-summaries:cor))
  ==
::
++  on-agent
  |=  [=wire =sign:agent:gall]
  ^-  (quip card _this)
  =^  cards  state
    abet:(agent:cor wire sign)
  [cards this]
::
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
  =.  next-group  1
  (emit [%pass /eyre/messages %arvo %e %connect [~ /messages] %messages])
::
++  load
  |=  =vase
  ^+  cor
  ::  a state that fits no known version crashes the upgrade, so Gall keeps the
  ::  old agent and its data, rather than starting over empty
  =/  old  !<(versioned-state:msg vase)
  =.  state
    ?-  -.old
      %2  old
      %1  (upgrade-1 old)
      %0  (upgrade-1 [%1 messages.old next-id.old read.old api-key.old ~ ~])
    ==
  (emit [%pass /eyre/messages %arvo %e %connect [~ /messages] %messages])
::
::  a state-1 save gains group conversations: no groups yet, the first id free
++  upgrade-1
  |=  old=state-1:msg
  ^-  state-2:msg
  :*  %2
      (~(run by messages.old) to-message)
      next-id.old
      read.old
      api-key.old
      blocked.old
      recent.old
      ~
      1
      ~
  ==
::
++  to-message
  |=  m=message-1:msg
  ^-  message:msg
  [id.m peer.m outgoing.m body.m sent.m status.m ~ ~]
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
      %noun
    (receive vase)
  ==
::
::  a message from another ship: store it as received. A poke over Ames
::  arrives as a vase of type *, so the noun is molded, not cast with !<.
::  Anything unreadable, empty or too long crashes, which nacks the poke
::  and the sender shows %failed. A message to ourselves is already stored
::  as outgoing, so its received copy is dropped. A blocked peer, or one past
::  rate-max messages in rate-window, is nacked the same way and stores nothing.
++  receive
  |=  =vase
  ^+  cor
  ?:  ?=([%grp *] q.vase)
    =/  [* gd=@t owner=@p title=@t members=(list @p) body=@t sent=@da]
      ;;([%grp @t @p @t (list @p) @t @da] q.vase)
    (receive-group gd owner title members body sent)
  =/  [* body=@t sent=@da]  ;;([%msg @t @da] q.vase)
  ?<  =('' body)
  ?>  (lte (met 3 body) max-body)
  ?:  =(our.bowl src.bowl)  cor
  =.  recent  (admit src.bowl)
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =.  messages  (~(put by messages) id [id src.bowl %.n body sent %received ~ ~])
  cor
::
::  a group message from one of its members. The first message of a group carries
::  the group's details, so we make our copy from them. The sender must be a member,
::  and so must we; an existing group keeps its own members.
++  receive-group
  |=  [gd=gid:msg owner=@p title=@t members=(list @p) body=@t sent=@da]
  ^+  cor
  ?<  =('' body)
  ?>  (lte (met 3 body) max-body)
  ?>  (lte (met 3 title) max-title)
  ?>  (lte (lent members) max-members)
  ?:  =(our.bowl src.bowl)  cor
  =/  mset=(set @p)  (silt members)
  ?>  (~(has in mset) our.bowl)
  ?>  (~(has in mset) src.bowl)
  =/  have=(unit group:msg)  (~(get by groups) gd)
  =/  grp=group:msg
    ?~  have  [gd title owner mset]
    ?>  (~(has in members.u.have) src.bowl)
    u.have
  =.  groups  (~(put by groups) gd grp)
  =.  recent  (admit src.bowl)
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =.  messages  (~(put by messages) id [id src.bowl %.n body sent %received `gd ~])
  cor
::
::  the rate limit and block list, for a message from `src`
++  admit
  |=  src=@p
  ^+  recent
  ?<  (~(has in blocked) src)
  ::  arrivals less than rate-window old still count toward the limit
  =/  window=(list @da)
    %+  skim  (~(gut by recent) src ~)
    |=  t=@da
    ?|((gth t now.bowl) (lth (sub now.bowl t) rate-window))
  ?>  (lth (lent window) rate-max)
  (~(put by recent) src [now.bowl window])
::
++  agent
  |=  [=wire =sign:agent:gall]
  ^+  cor
  ?:  ?&(?=([%send @ ~] wire) ?=(%poke-ack -.sign))
    =/  id=@ud  (slav %ud i.t.wire)
    ?~  m=(~(get by messages) id)  cor
    =/  st=status:msg  ?~(p.sign %delivered %failed)
    =.  messages  (~(put by messages) id u.m(status st))
    cor
  ::  one member's ack for a group message: it is delivered once every member has acked
  ?:  ?&(?=([%gsend @ @ ~] wire) ?=(%poke-ack -.sign))
    =/  id=@ud  (slav %ud i.t.wire)
    =/  who=@p  (slav %p i.t.t.wire)
    ?~  m=(~(get by messages) id)  cor
    =/  left=(set @p)  (~(del in waiting.u.m) who)
    =/  nxt=status:msg
      ?^  p.sign  %failed
      ?:  =(%failed status.u.m)  %failed
      ?:  =(~ left)  %delivered
      %pending
    =.  messages  (~(put by messages) id u.m(status nxt, waiting left))
    cor
  cor
::
++  serve-http
  |=  [eyre-id=@ta =inbound-request:eyre]
  ^+  cor
  =/  =request-line:server
    (parse-request-line:server url.request.inbound-request)
  =*  site  site.request-line
  =/  method=@tas  method.request.inbound-request
  ?:  &(=(site ~[%messages %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  ?:  ?=([%messages %~.~ %v1 %conversations ~] site)
    ?.  =(%'GET' method)
      (http-error eyre-id 405 'method not allowed')
    %+  give-json-status  eyre-id
    (ok:enjs:messages-json [conversations+(conversations:enjs:messages-json summaries)]~)
  ?:  ?=([%messages %~.~ %v1 %conversations @ ~] site)
    ?~  who=(slaw %p i.t.t.t.t.site)
      (http-error eyre-id 400 'bad ship name')
    ?.  =(%'GET' method)
      (http-error eyre-id 405 'method not allowed')
    %+  give-json-status  eyre-id
    (ok:enjs:messages-json [messages+(messages:enjs:messages-json (thread u.who))]~)
  ?:  ?=([%messages %~.~ %v1 %conversations @ %read ~] site)
    ?~  who=(slaw %p i.t.t.t.t.site)
      (http-error eyre-id 400 'bad ship name')
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (dispatch 'conversation.read' (pairs:enjs:format ~[peer+s+(scot %p u.who)]))
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %conversations @ %block ~] site)
    ?~  who=(slaw %p i.t.t.t.t.site)
      (http-error eyre-id 400 'bad ship name')
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (dispatch 'peer.block' (pairs:enjs:format ~[peer+s+(scot %p u.who)]))
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %conversations @ %unblock ~] site)
    ?~  who=(slaw %p i.t.t.t.t.site)
      (http-error eyre-id 400 'bad ship name')
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (dispatch 'peer.unblock' (pairs:enjs:format ~[peer+s+(scot %p u.who)]))
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %messages ~] site)
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (body-json inbound-request 'message.send')
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %messages @ ~] site)
    ?~  id=(parse-id i.t.t.t.t.site)
      (give-json-status eyre-id (err:enjs:messages-json 'bad-request' 'bad id'))
    ?.  =(%'DELETE' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (dispatch 'message.delete' (pairs:enjs:format ~[id+(numb:enjs:format u.id)]))
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %groups ~] site)
    ?:  =(%'GET' method)
      (give-json-status eyre-id (ok:enjs:messages-json [groups+(groups:enjs:messages-json group-summaries)]~))
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (body-json inbound-request 'group.new')
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %groups @ ~] site)
    ?.  =(%'GET' method)
      (http-error eyre-id 405 'method not allowed')
    %+  give-json-status  eyre-id
    (ok:enjs:messages-json [messages+(messages:enjs:messages-json (group-thread i.t.t.t.t.site))]~)
  ?:  ?=([%messages %~.~ %v1 %groups @ %messages ~] site)
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    ?~  body.request.inbound-request
      (give-json-status eyre-id (err:enjs:messages-json 'bad-request' 'missing body'))
    ?~  jon=(de:json:html q.u.body.request.inbound-request)
      (give-json-status eyre-id (err:enjs:messages-json 'bad-request' 'invalid json'))
    ?.  ?=(%o -.u.jon)
      (give-json-status eyre-id (err:enjs:messages-json 'bad-request' 'need a json object'))
    =^  out=json  cor
      (dispatch 'group.send' [%o (~(put by p.u.jon) 'group' [%s i.t.t.t.t.site])])
    (give-json-status eyre-id out)
  ?:  ?=([%messages %~.~ %v1 %groups @ %read ~] site)
    ?.  =(%'POST' method)
      (http-error eyre-id 405 'method not allowed')
    =^  out=json  cor  (dispatch 'group.read' (pairs:enjs:format ~[group+s+i.t.t.t.t.site]))
    (give-json-status eyre-id out)
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
  (give-http eyre-id (status-of:messages-json json) 'application/json' (en:json:html json))
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
::  plain digits ("1234567") or Hoon's dotted form ("1.234.567")
++  parse-id
  |=  t=@t
  ^-  (unit @ud)
  =/  plain=(unit @ud)  (rush t dem)
  ?^  plain  plain
  (slaw %ud t)
::
++  bad
  |=  msg=@t
  ^-  [json _cor]
  [(err:enjs:messages-json 'bad-request' msg) cor]
::
++  body-json
  |=  [req=inbound-request:eyre fn=@t]
  ^-  [json _cor]
  ?~  body.request.req
    [(err:enjs:messages-json 'bad-request' 'missing body') cor]
  ?~  jon=(de:json:html q.u.body.request.req)
    [(err:enjs:messages-json 'bad-request' 'invalid json') cor]
  (dispatch fn u.jon)
::
++  run-json
  |=  jon=json
  ^-  [json _cor]
  =/  parsed=(each [@t json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:messages-json 'bad-request' 'malformed call') cor]
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
  ?:  =(fn 'message.send')          (send-message args)
  ?:  =(fn 'message.delete')        (delete-message args)
  ?:  =(fn 'conversation.read')     (read-conversation args)
  ?:  =(fn 'peer.block')            (set-block args %.y)
  ?:  =(fn 'peer.unblock')          (set-block args %.n)
  ?:  =(fn 'group.new')             (new-group args)
  ?:  =(fn 'group.send')            (send-group args)
  ?:  =(fn 'group.read')            (read-group args)
  [(err:enjs:messages-json 'unknown-function' fn) cor]
::
::  all direct messages with one ship, oldest first
++  thread
  |=  who=@p
  ^-  (list message:msg)
  %+  sort  (skim ~(val by messages) |=(m=message:msg &(=(peer.m who) ?=(~ group.m))))
  |=  [a=message:msg b=message:msg]
  (lth id.a id.b)
::
::  all messages of one group, oldest first
++  group-thread
  |=  gd=gid:msg
  ^-  (list message:msg)
  %+  sort  (skim ~(val by messages) |=(m=message:msg =(group.m `gd)))
  |=  [a=message:msg b=message:msg]
  (lth id.a id.b)
::
::  one row per peer: how many messages, how many unread, and the last one.
::  Newest conversation first (ids only grow, so the highest is the newest).
++  summaries
  ^-  (list summary)
  =/  peers=(list @p)
    ~(tap in (silt (turn (skim ~(val by messages) |=(m=message:msg ?=(~ group.m))) |=(m=message:msg peer.m))))
  %+  sort  (turn peers summary-of)
  |=  [a=summary b=summary]
  (gth ?~(last.a 0 id.u.last.a) ?~(last.b 0 id.u.last.b))
::
+$  summary  [peer=@p count=@ud unread=@ud last=(unit message:msg) blocked=?]
++  summary-of
  |=  who=@p
  ^-  summary
  =/  mine=(list message:msg)  (thread who)
  =/  seen=@ud  (~(gut by read) who 0)
  =/  unread=@ud
    (lent (skim mine |=(m=message:msg &(!outgoing.m (gth id.m seen)))))
  [who (lent mine) unread ?~(mine ~ `(rear mine)) (~(has in blocked) who)]
::
::  one row per group we belong to, newest activity first
+$  gsummary  [grp=group:msg count=@ud unread=@ud last=(unit message:msg)]
++  group-summaries
  ^-  (list gsummary)
  %+  sort  (turn ~(val by groups) group-summary-of)
  |=  [a=gsummary b=gsummary]
  (gth ?~(last.a 0 id.u.last.a) ?~(last.b 0 id.u.last.b))
::
++  group-summary-of
  |=  grp=group:msg
  ^-  gsummary
  =/  mine=(list message:msg)  (group-thread gid.grp)
  =/  seen=@ud  (~(gut by group-read) gid.grp 0)
  =/  unread=@ud
    (lent (skim mine |=(m=message:msg &(!outgoing.m (gth id.m seen)))))
  [grp (lent mine) unread ?~(mine ~ `(rear mine))]
::
++  parse-send
  |=  args=json
  ^-  [@p @t]
  ?>  ?=(%o -.args)
  :-  (slav %p (so:dejs:format (~(got by p.args) 'to')))
  (so:dejs:format (~(got by p.args) 'body'))
::
++  send-message
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@p @t] tang)  (mule |.((parse-send args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:messages-json 'bad-request' 'need to (a ship, like ~zod) and body') cor]
  =/  [to=@p body=@t]  p.parsed
  ?:  =('' body)
    [(err:enjs:messages-json 'bad-request' 'message is empty') cor]
  ?:  (gth (met 3 body) max-body)
    [(err:enjs:messages-json 'bad-request' 'message is too long') cor]
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =.  messages  (~(put by messages) id [id to %.y body now.bowl %pending ~ ~])
  =/  out=_cor
    (emit [%pass /send/(scot %ud id) %agent [to %messages] %poke %noun !>([%msg body now.bowl])])
  [(ok:enjs:messages-json [id+(numb:enjs:format id)]~) out]
::
++  parse-new-group
  |=  args=json
  ^-  [@t (list @p)]
  ?>  ?=(%o -.args)
  =/  names=(list @t)
    ((ar:dejs:format so:dejs:format) (~(got by p.args) 'members'))
  :-  (so:dejs:format (~(got by p.args) 'title'))
  (turn names |=(t=@t (slav %p t)))
::
::  start a group with us as its owner. Other members learn of it from its first
::  message, which carries the title and the member list.
++  new-group
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [@t (list @p)] tang)  (mule |.((parse-new-group args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:messages-json 'bad-request' 'need a title and members (ships like ~zod)') cor]
  =/  [title=@t members=(list @p)]  p.parsed
  ?:  =('' title)
    [(err:enjs:messages-json 'bad-request' 'title is empty') cor]
  ?:  (gth (met 3 title) max-title)
    [(err:enjs:messages-json 'bad-request' 'title is too long') cor]
  ?:  (gth (lent members) (dec max-members))
    [(err:enjs:messages-json 'bad-request' 'too many members') cor]
  =/  mset=(set @p)  (~(put in (silt members)) our.bowl)
  ?:  =(1 ~(wyt in mset))
    [(err:enjs:messages-json 'bad-request' 'add at least one other member') cor]
  =/  gd=gid:msg  (rap 3 ~[(scot %p our.bowl) '-' (scot %ud next-group)])
  =.  groups  (~(put by groups) gd [gd title our.bowl mset])
  =.  next-group  +(next-group)
  [(ok:enjs:messages-json [gid+s+gd]~) cor]
::
++  parse-group-send
  |=  args=json
  ^-  [gid:msg @t]
  ?>  ?=(%o -.args)
  :-  (so:dejs:format (~(got by p.args) 'group'))
  (so:dejs:format (~(got by p.args) 'body'))
::
::  write to a group: one copy here, and a poke to each other member. The message
::  is %pending until every member has acked it.
++  send-group
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each [gid:msg @t] tang)  (mule |.((parse-group-send args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:messages-json 'bad-request' 'need a group and a body') cor]
  =/  [gd=gid:msg body=@t]  p.parsed
  ?:  =('' body)
    [(err:enjs:messages-json 'bad-request' 'message is empty') cor]
  ?:  (gth (met 3 body) max-body)
    [(err:enjs:messages-json 'bad-request' 'message is too long') cor]
  ?~  have=(~(get by groups) gd)
    [(err:enjs:messages-json 'not-found' 'no such group') cor]
  =/  grp=group:msg  u.have
  =/  others=(list @p)  ~(tap in (~(del in members.grp) our.bowl))
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  st=status:msg  ?~(others %delivered %pending)
  =.  messages
    (~(put by messages) id [id our.bowl %.y body now.bowl st `gd (silt others)])
  =/  cds=(list card)
    %+  turn  others
    |=  who=@p
    ^-  card
    :*  %pass  /gsend/(scot %ud id)/(scot %p who)  %agent  [who %messages]  %poke  %noun
        !>([%grp gd owner.grp title.grp ~(tap in members.grp) body now.bowl])
    ==
  [(ok:enjs:messages-json [id+(numb:enjs:format id)]~) (emil cds)]
::
::  mark a group read through its newest message
++  read-group
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each gid:msg tang)
    (mule |.(?>(?=(%o -.args) (so:dejs:format (~(got by p.args) 'group')))))
  ?:  ?=(%| -.parsed)
    [(err:enjs:messages-json 'bad-request' 'need group') cor]
  =/  mine=(list message:msg)  (group-thread p.parsed)
  ?~  mine
    [(ok:enjs:messages-json ~) cor]
  =/  last=message:msg  (rear mine)
  =/  cur=@ud  (~(gut by group-read) p.parsed 0)
  =.  group-read
    ?:((gth id.last cur) (~(put by group-read) p.parsed id.last) group-read)
  [(ok:enjs:messages-json ~) cor]
::
++  delete-message
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each @ud tang)
    (mule |.(?>(?=(%o -.args) (ni:dejs:format (~(got by p.args) 'id')))))
  ?:  ?=(%| -.parsed)
    (bad 'need id')
  ?.  (~(has by messages) p.parsed)
    [(err:enjs:messages-json 'not-found' 'no such message') cor]
  =.  messages  (~(del by messages) p.parsed)
  [(ok:enjs:messages-json [id+(numb:enjs:format p.parsed)]~) cor]
::
++  parse-peer
  |=  args=json
  ^-  @p
  ?>  ?=(%o -.args)
  (slav %p (so:dejs:format (~(got by p.args) 'peer')))
::
::  mark everything from `peer` as read, through the newest message
++  read-conversation
  |=  args=json
  ^-  [json _cor]
  =/  parsed=(each @p tang)  (mule |.((parse-peer args)))
  ?:  ?=(%| -.parsed)
    (bad 'need peer, a ship like ~zod')
  =/  mine=(list message:msg)  (thread p.parsed)
  ?~  mine
    [(ok:enjs:messages-json ~) cor]
  =/  last=message:msg  (rear mine)
  =/  cur=@ud  (~(gut by read) p.parsed 0)
  =.  read  ?:((gth id.last cur) (~(put by read) p.parsed id.last) read)
  [(ok:enjs:messages-json ~) cor]
::
::  refuse incoming messages from `peer` (block), or accept them again
::  (unblock). Idempotent; stored messages and the rate history are kept.
++  set-block
  |=  [args=json block=?]
  ^-  [json _cor]
  =/  parsed=(each @p tang)  (mule |.((parse-peer args)))
  ?:  ?=(%| -.parsed)
    (bad 'need peer, a ship like ~zod')
  =.  blocked
    ?:  block  (~(put in blocked) p.parsed)
    (~(del in blocked) p.parsed)
  [(ok:enjs:messages-json ~[peer+s+(scot %p p.parsed) blocked+b+block]) cor]
--
