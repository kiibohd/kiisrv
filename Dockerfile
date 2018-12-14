FROM build-template

ARG TAG=master

RUN git clone https://github.com/kiibohd/controller.git -b $TAG
RUN cd controller/Keyboards; pipenv install
RUN cd controller/Keyboards; pipenv run /usr/local/bin/update_kll_cache.sh

WORKDIR /controller/Keyboards
