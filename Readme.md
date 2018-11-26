# Balthazar

## Balthacephalo

## Balthapode

## Balthmessage

## ...

## TODO

- Check evolution of wasmi : [wasmi on github](https://github.com/paritytech/wasmi)

## Message forwarding solutions :

- ~~Send a request to find peer and directly create a connection to it~~
    - What if it can't be connected to (proxy, nat, ...) ?
    - How do I know it doesn't exist ?
- ~~Send a request to know if peer is somewhere on the network~~
    - It can disconnect before the message is actually sent
    - How do I know it doesn't exist ?
- Send the message in a special Forward msg to every other peers :
    // - If the receiving peer is the target, send Ack or send the answer directly ?
    - If the receiving peer knows the target, it sends forwards directly the message to it.
        - Send `Found` back
    - If not, forwards to every other peer (not sender) :
        - Wait for all answers : If one `Found` : return `Found`, else return `NotFound`

- Keep track of the path by sending along a growing list ?
    - Same for broadcast ?

0 - 1
  \ | 
    2 - 3