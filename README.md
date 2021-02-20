Building images:

```
make images
```

Running the system (server, test agent and frontend):

```
docker-compose up
```

Making toolchain images:

```
make toolchains/[name]/cid
```

Running toolchain:

```
make run-toolchain-[name]
```