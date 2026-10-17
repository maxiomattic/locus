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
--
