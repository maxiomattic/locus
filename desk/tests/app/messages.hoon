::  Messenger — receive from a peer, read, summarise, and reject empty sends.
::  Spam protection: blocked peers and the per-peer rate limit, and the
::  upgrade of a state-0 save.
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
::
::  a JSON call about one peer, like peer.block or peer.unblock
++  about
  |=  [fn=@t who=@t]
  %-  pairs:enjs:format
  :~  fn+s+fn
      args+(pairs:enjs:format ~[peer+s+who])
  ==
::
::  the conversation list, as text
++  listing
  =/  m  (mare ,tape)
  ^-  form:m
  ;<  =cage  bind:m  (got-peek /x/messages)
  (pure:m (trip (en:json:html !<(json q.cage))))
::
++  has
  |=  [txt=tape pat=@t]
  ^-  ?
  ?=(^ (find (trip pat) txt))
::
::  `n` messages from the current src, all accepted
++  remote-n
  |=  n=@ud
  =/  m  (mare ,~)
  ^-  form:m
  ?:  =(0 n)  (pure:m ~)
  ;<  *  bind:m  (remote 'hi' ~2026.10.16)
  (remote-n (dec n))
::
++  test-blocked-peer-is-nacked-and-not-stored
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'before' ~2026.10.16)
  ;<  ~  bind:m  (set-src ~zod)
  ;<  *  bind:m  (call (about 'peer.block' '~dev'))
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (ex-fail (remote 'spam' ~2026.10.16))
  ;<  txt=tape  bind:m  listing
  ?<  (has txt '"body":"spam"')
  ?>  (has txt '"count":1')
  ?>  (has txt '"blocked":true')
  (pure:m ~)
::
++  test-block-survives-reload
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call (about 'peer.block' '~dev'))
  ;<  *  bind:m  (do-load messages-agent ~)
  ;<  ~  bind:m  (set-src ~dev)
  (ex-fail (remote 'spam' ~2026.10.16))
::
++  test-block-does-not-stop-our-sends
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call (about 'peer.block' '~dev'))
  ;<  *  bind:m
    %-  call
    %-  pairs:enjs:format
    :~  fn+s+'message.send'
        args+(pairs:enjs:format ~[to+s+'~dev' body+s+'still mine'])
    ==
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"body":"still mine"')
  (pure:m ~)
::
++  test-unblock-lets-messages-in
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call (about 'peer.block' '~dev'))
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (ex-fail (remote 'spam' ~2026.10.16))
  ;<  ~  bind:m  (set-src ~zod)
  ;<  *  bind:m  (call (about 'peer.unblock' '~dev'))
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'welcome back' ~2026.10.16)
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"body":"welcome back"')
  ?>  (has txt '"blocked":false')
  (pure:m ~)
::
::  only our own ship may block
++  test-peer-cannot-block
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  (ex-fail (call (about 'peer.block' '~nec')))
::
++  test-bad-block-args-are-an-error
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call (about 'peer.block' 'not-a-ship'))
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'still fine' ~2026.10.16)
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"body":"still fine"')
  (pure:m ~)
::
++  test-twenty-first-in-a-minute-is-nacked
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (remote-n 20)
  ;<  ~  bind:m  (ex-fail (remote 'one too many' ~2026.10.16))
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"count":20')
  ?<  (has txt '"body":"one too many"')
  (pure:m ~)
::
::  the window is a rolling 60 seconds: a message 59s after a full minute's
::  worth is refused, and one 60s after is taken
++  test-accepted-again-after-the-minute
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (wait ~d1)
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (remote-n 20)
  ;<  ~  bind:m  (wait ~s59)
  ;<  ~  bind:m  (ex-fail (remote 'too soon' ~2026.10.16))
  ;<  ~  bind:m  (wait ~s1)
  ;<  *  bind:m  (remote 'later' ~2026.10.16)
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"body":"later"')
  ?>  (has txt '"count":21')
  (pure:m ~)
::
::  the window rolls: messages spread over time leave room as the old ones age
++  test-window-rolls
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (wait ~d1)
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (remote-n 10)
  ;<  ~  bind:m  (wait ~s30)
  ;<  ~  bind:m  (remote-n 10)
  ;<  ~  bind:m  (ex-fail (remote 'full' ~2026.10.16))
  ;<  ~  bind:m  (wait ~s30)
  ;<  ~  bind:m  (remote-n 10)
  ;<  ~  bind:m  (ex-fail (remote 'full again' ~2026.10.16))
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"count":30')
  (pure:m ~)
::
++  test-rate-limit-is-per-peer
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (remote-n 20)
  ;<  ~  bind:m  (set-src ~nec)
  ;<  *  bind:m  (remote 'from nec' ~2026.10.16)
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"body":"from nec"')
  (pure:m ~)
::
::  a save from before blocking existed loads, keeps its data, and starts
::  with nobody blocked and no rate history
++  team-new
  %-  pairs:enjs:format
  :~  fn+s+'group.new'
      args+(pairs:enjs:format ~[title+s+'Team' members+a+~[s+'~dev']])
  ==
::
++  team-send
  %-  pairs:enjs:format
  :~  fn+s+'group.send'
      args+(pairs:enjs:format ~[group+s+'~zod-1' body+s+'hi team'])
  ==
::
++  team-read
  %-  pairs:enjs:format
  :~  fn+s+'group.read'
      args+(pairs:enjs:format ~[group+s+'~dev-1'])
  ==
::
++  test-group-message-from-member-is-received
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (do-poke %noun !>(`*`[%grp '~dev-1' ~dev 'Team' ~[~zod ~dev] 'hello group' `@da`~2026.10.16]))
  ;<  =cage  bind:m  (got-peek /x/groups)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?>  (has txt '"gid":"~dev-1"')
  ?>  (has txt '"title":"Team"')
  ?>  (has txt '"unread":1')
  (pure:m ~)
::
++  test-group-from-non-member-is-nacked
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  ~  bind:m  (ex-fail (do-poke %noun !>(`*`[%grp '~nec-1' ~nec 'Other' ~[~zod ~nec] 'x' `@da`~2026.10.16])))
  ;<  =cage  bind:m  (got-peek /x/groups)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?<  (has txt '"gid":"~nec-1"')
  (pure:m ~)
::
++  test-group-send-waits-for-members
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call team-new)
  ;<  *  bind:m  (call team-send)
  ;<  =cage  bind:m  (got-peek /x/groups)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?>  (has txt '"gid":"~zod-1"')
  ?>  (has txt '"count":1')
  ?>  (has txt '"status":"pending"')
  ?>  (has txt '"waiting":["~dev"]')
  (pure:m ~)
::
++  test-group-read-clears-unread
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (do-poke %noun !>(`*`[%grp '~dev-1' ~dev 'Team' ~[~zod ~dev] 'hello group' `@da`~2026.10.16]))
  ;<  ~  bind:m  (set-src ~zod)
  ;<  *  bind:m  (call team-read)
  ;<  =cage  bind:m  (got-peek /x/groups)
  =/  txt=tape  (trip (en:json:html !<(json q.cage)))
  ?>  (has txt '"unread":0')
  (pure:m ~)

++  test-state-0-loads
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  =/  old=state-0:msg
    :*  %0
        (~(put by *(map @ud message-1:msg)) 7 [7 ~dev %.n 'kept' ~2026.10.16 %received])
        8
        (~(put by *(map @p @ud)) ~dev 7)
        `'old-key'
    ==
  ;<  *  bind:m  (do-load messages-agent `!>(old))
  ;<  sav=vase  bind:m  get-save
  =/  new=state-2:msg  !<(state-2:msg sav)
  ?>  =(%2 -.new)
  ?>  =(~ blocked.new)
  ?>  =(~ recent.new)
  ?>  =(8 next-id.new)
  ?>  =(`'old-key' api-key.new)
  ?>  =(`7 (~(get by read.new) ~dev))
  ;<  txt=tape  bind:m  listing
  ?>  (has txt '"body":"kept"')
  ?>  (has txt '"blocked":false')
  ;<  ~  bind:m  (set-src ~dev)
  ;<  *  bind:m  (remote 'after upgrade' ~2026.10.16)
  ;<  sav=vase  bind:m  get-save
  =/  newer=state-2:msg  !<(state-2:msg sav)
  ?>  (~(has by messages.newer) 8)
  ?>  =(9 next-id.newer)
  (pure:m ~)
::

--
