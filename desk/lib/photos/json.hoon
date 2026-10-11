/-  pho=photos
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
  ++  photo
    |=  p=photo:pho
    ^-  json
    %-  pairs
    :~  id+(numb id.p)
        album+s+album.p
        title+s+title.p
        path+s+path.p
        tags+a+(turn tags.p |=(t=@t s+t))
        revision+(numb revision.p)
    ==
  ::
  ++  photos
    |=  m=(map @ud photo:pho)
    ^-  json
    a+(turn ~(val by m) photo)
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
