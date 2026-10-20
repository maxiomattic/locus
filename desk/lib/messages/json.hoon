/-  msg=messages
|%
++  status-of
  |=  jon=json
  ^-  @ud
  ?.  ?=(%o -.jon)  200
  ?~  e=(~(get by p.jon) 'error')  200
  =/  typ=@t  (so:dejs:format u.e)
  ?+  typ  200
    %bad-request  400
    %not-found    404
    %conflict     409
  ==
::
++  enjs
  =,  enjs:format
  |%
  ++  message
    |=  m=message:msg
    ^-  json
    %-  pairs
    :~  id+(numb id.m)
        peer+s+(scot %p peer.m)
        outgoing+b+outgoing.m
        body+s+body.m
        sent+s+(scot %da sent.m)
        status+s+status.m
        group+?~(group.m ~ s+u.group.m)
        waiting+a+(turn ~(tap in waiting.m) |=(w=@p s+(scot %p w)))
    ==
  ::
  ++  messages
    |=  l=(list message:msg)
    ^-  json
    a+(turn l message)
  ::
  ++  conversation
    |=  [peer=@p count=@ud unread=@ud last=(unit message:msg) blocked=?]
    ^-  json
    %-  pairs
    :~  peer+s+(scot %p peer)
        count+(numb count)
        unread+(numb unread)
        last+?~(last ~ (message u.last))
        blocked+b+blocked
    ==
  ::
  ++  conversations
    |=  l=(list [peer=@p count=@ud unread=@ud last=(unit message:msg) blocked=?])
    ^-  json
    a+(turn l |=([peer=@p count=@ud unread=@ud last=(unit message:msg) blocked=?] (conversation peer count unread last blocked)))
  ::
  ++  group
    |=  [grp=group:msg count=@ud unread=@ud last=(unit message:msg)]
    ^-  json
    %-  pairs
    :~  gid+s+gid.grp
        title+s+title.grp
        owner+s+(scot %p owner.grp)
        members+a+(turn ~(tap in members.grp) |=(w=@p s+(scot %p w)))
        count+(numb count)
        unread+(numb unread)
        last+?~(last ~ (message u.last))
    ==
  ::
  ++  groups
    |=  l=(list [grp=group:msg count=@ud unread=@ud last=(unit message:msg)])
    ^-  json
    a+(turn l |=([grp=group:msg count=@ud unread=@ud last=(unit message:msg)] (group grp count unread last)))
  ::
  ++  ok
    |=  extra=(list [@t json])
    ^-  json
    (pairs [[%ok b+&] extra])
  ::
  ++  err
    |=  [typ=@t msg=@t]
    ^-  json
    %-  pairs
    :~  ok+b+|
        error+s+typ
        message+s+msg
    ==
  --
--
