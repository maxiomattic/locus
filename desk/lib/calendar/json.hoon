/-  cal=calendar
|%
++  enjs
  =,  enjs:format
  |%
  ++  event
    |=  e=event:cal
    ^-  json
    %-  pairs
    :~  id+(numb id.e)
        calendar+s+calendar.e
        title+s+title.e
        start+(numb (unt:chrono:userlib start.e))
        end+(numb (unt:chrono:userlib end.e))
        body+s+body.e
        revision+(numb revision.e)
    ==
  ::
  ++  events
    |=  m=(map @ud event:cal)
    ^-  json
    a+(turn ~(val by m) event)
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
