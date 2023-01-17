# /bin/sh
file=$(pwd)/initrd/ksymbols
: > $file
i=0
while read line
do
    arr=($line);
    echo ${arr[0]} | xxd -r -p | xxd -g 8 -e | xxd -r >> $file
    echo -n ${arr[2]} >> $file
    echo -n -e "\x00" >> $file
    let "i+=1"
done