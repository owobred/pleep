diff new_songlist.csv songlist/songlist.csv > /dev/null
exit_code=$?

if [ $exit_code -eq 0 ]
then
    echo "songlist did not change"
    echo "songlist_updated=false" >> "$GITHUB_OUTPUT"
else
    echo "songlist changed"
    mv new_songlist.csv songlist/songlist.csv
    cd songlist

    git config --local user.email "worker@github.com"
    git config --local user.name "songlist worker"

    git add songlist.csv
    git commit -m "update songlist"
    git push origin songlist-history
    echo "songlist_updated=true" >> "$GITHUB_OUTPUT"
fi