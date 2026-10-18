::  Messenger — receive from a peer, read, summarise, and reject empty sends.
::
/-  msg=messages
/+  *test-agent
/=  messages-agent  /app/messages
|%
++  dap  %messages
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap messages-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  read-from-dev
  %-  pairs:enjs:format
  :~  fn+s+'conversation.read'
      args+(pairs:enjs:format ~[peer+s+'~dev'])
  ==
::
++  send-empty
  %-  pairs:enjs:format
  :~  fn+s+'message.send'
      args+(pairs:enjs:format ~[to+s+'~dev' body+s+''])
  ==
::
++  test-receive-is-unread
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (do-poke %noun !>([%msg 'hello' `@da`~2026.10.16]))
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?>  ?=(^ (find (trip '"peer":"~dev"') txt))
  ?>  ?=(^ (find (trip '"unread":1') txt))
  ?>  ?=(^ (find (trip '"body":"hello"') txt))
  (pure:m ~)
::
++  test-read-clears-unread
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (do-poke %noun !>([%msg 'hello' `@da`~2026.10.16]))
  ;<  ~  bind:m  (set-src ~zod)
  ;<  *  bind:m  (call read-from-dev)
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?>  ?=(^ (find (trip '"unread":0') txt))
  (pure:m ~)
::
++  test-empty-send-is-rejected
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call send-empty)
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?<  ?=(^ (find (trip '"~dev"') txt))
  (pure:m ~)
::
::  a poke from another ship arrives over Ames as an untyped noun
++  remote
  |=  [body=@t sent=@da]
  (do-poke %noun !>(`*`[%msg body sent]))
::
++  test-untyped-remote-poke-is-stored
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'over ames' ~2026.10.16)
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?>  ?=(^ (find (trip '"body":"over ames"') txt))
  (pure:m ~)
::
++  test-unreadable-poke-is-nacked
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  (ex-fail (do-poke %noun !>(`*`[%hello 42])))
::
++  test-too-long-body-is-nacked
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  =/  long=@t  (crip (reap 8.193 'x'))
  (ex-fail (remote long ~2026.10.16))
::
++  test-own-copy-is-not-stored-twice
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (remote 'to myself' ~2026.10.16)
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?<  ?=(^ (find (trip '"body":"to myself"') txt))
  (pure:m ~)
::
++  test-newest-conversation-first
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'first' ~2026.10.16)
  ;<  ~  bind:m  (set-src ~nec)
  ;<  *  bind:m  (remote 'second' ~2026.10.16)
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'third' ~2026.10.16)
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  =/  dev=(unit @ud)  (find (trip '"peer":"~dev"') txt)
  =/  nec=(unit @ud)  (find (trip '"peer":"~nec"') txt)
  ?>  &(?=(^ dev) ?=(^ nec) (lth u.dev u.nec))
  (pure:m ~)
--
