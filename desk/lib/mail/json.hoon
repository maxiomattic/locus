/-  mail
|%
++  enjs
  =,  enjs:format
  |%
  ++  folder
    |=  f=folder:mail
    ^-  json
    s+f
  ::
  ++  message
    |=  m=message:mail
    ^-  json
    %-  pairs
    :~  id+(numb id.m)
        folder+(folder folder.m)
        from+s+from.m
        to+a+(turn to.m |=(t=@t s+t))
        subject+s+subject.m
        body+s+body.m
        at+(time at.m)
        revision+(numb revision.m)
    ==
  ::
  ++  messages
    |=  m=(map @ud message:mail)
    ^-  json
    a+(turn ~(val by m) message)
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
++  dejs
  =,  dejs:format
  |%
  ++  folder
    |=  jon=json
    ^-  folder:mail
    =/  t=@t  (so jon)
    ?:  =(t 'inbox')   %inbox
    ?:  =(t 'sent')    %sent
    ?:  =(t 'drafts')  %drafts
    ?:  =(t 'trash')   %trash
    ~|(bad-folder+t !!)
  --
--
