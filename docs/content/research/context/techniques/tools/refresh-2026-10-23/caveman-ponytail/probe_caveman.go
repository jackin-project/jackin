package main

import (
 "bytes"
 "encoding/json"
 "fmt"
 "os"
 "time"
 "github.com/JuliusBrussee/caveman/engine"
 "github.com/JuliusBrussee/caveman/engine/ccr"
 "github.com/JuliusBrussee/caveman/engine/tokens"
)

func main() {
 out:=map[string]any{}
 counts:=map[string]any{}
 for _,p:=range os.Args[1:] { b,e:=os.ReadFile(p);if e!=nil {panic(e)};counts[p]=map[string]any{"bytes":len(b),"o200k_base":tokens.Default().Count(b)} }
 out["instruction_counts"]=counts
 data:=[]byte("[\r\n"+string(bytes.Repeat([]byte("{\"state\":\"ok\",\"service\":\"λ雪\",\"value\":7},\r\n"),999))+"{\"state\":\"rare\",\"service\":\"critical\",\"value\":-3}]")
 store,err:=ccr.OpenMemory();if err!=nil {panic(err)};defer store.Close()
 e:=engine.New(store,nil)
 raw,err:=e.Compress(data,engine.Options{});if err!=nil {panic(err)}
 out["default_record_exact"]=bytes.Equal(data,raw.Output)
 noStore,err:=engine.New(nil,nil).Compress(data,engine.Options{Mode:engine.ModeCompress});if err!=nil {panic(err)}
 out["no_store_passes_original"]=bytes.Equal(data,noStore.Output)
 times:=[]float64{}; var r engine.Result
 for i:=0;i<11;i++ { t:=time.Now();r,err=e.Compress(data,engine.Options{Mode:engine.ModeCompress,Query:"critical rare value"});if err!=nil {panic(err)};times=append(times,float64(time.Since(t).Microseconds())/1000) }
 recovered,err:=e.Retrieve(r.RecoveryHandle);if err!=nil {panic(err)}
 out["fixture"]=map[string]any{"bytes":len(data),"content_type":r.ContentType,"tokens_before":r.TokensBefore,"tokens_after":r.TokensAfter,"handle":r.RecoveryHandle,"byte_exact_recovery_crlf_unicode_no_final_newline":bytes.Equal(data,recovered),"compression_ms":times}
 malformed:=[]byte("[{\"a\":1,]")
 r,err=e.Compress(malformed,engine.Options{Mode:engine.ModeCompress,Type:"json"});if err!=nil {panic(err)}
 out["malformed_json_passes_original"]=bytes.Equal(malformed,r.Output)
 _,err=e.Retrieve("ccr_00000000000000000000000000000000");out["missing_recovery_is_error"]=err!=nil
 b,err:=json.MarshalIndent(out,"","  ");if err!=nil {panic(err)};fmt.Println(string(b))
}
