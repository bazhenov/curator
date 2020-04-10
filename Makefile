.PHONY: clean

%.iid: src/*
	docker build --target=$(basename $@) --iidfile $@ -t $(basename $@) .

clean:
	rm -f *.iid