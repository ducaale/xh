# Test Fixtures: Compressed Responses

```sh
$ echo "Hello world" > hello_world
$ pigz hello_world # hello_world.gz

$ echo "Hello world" > hello_world
$ pigz -z hello_world # hello_world.zz

$ echo "Hello world" > hello_world
$ brotli hello_world # hello_world.br
```
