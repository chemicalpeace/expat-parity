#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "expat.h"
static void XMLCALL st(void*u,const XML_Char*n,const XML_Char**a){(void)u;(void)n;(void)a;}
static void XMLCALL en(void*u,const XML_Char*n){(void)u;(void)n;}
int main(int argc,char**argv){
  /* argv[1]=file argv[2]=ns(0/1) argv[3]=chunk */
  FILE*f=fopen(argv[1],"rb"); fseek(f,0,SEEK_END); long n=ftell(f); fseek(f,0,SEEK_SET);
  char*b=malloc(n+1); fread(b,1,n,f); fclose(f);
  int ns=atoi(argv[2]); long chunk=argc>3?atol(argv[3]):0;
  XML_Parser p= ns? XML_ParserCreateNS(NULL,'|') : XML_ParserCreate(NULL);
  if(ns>1) XML_SetReturnNSTriplet(p,1);
  XML_SetElementHandler(p,st,en);
  XML_SetHashSalt(p,1);
  XML_SetParamEntityParsing(p,XML_PARAM_ENTITY_PARSING_ALWAYS);
  int r=1;
  if(!chunk) r=XML_Parse(p,b,(int)n,1);
  else { for(long i=0;i<n && r;i+=chunk){ long m=(n-i<chunk)?n-i:chunk; r=XML_Parse(p,b+i,(int)m,0);} if(r) r=XML_Parse(p,"",0,1);}
  fprintf(stderr,"RESULT %d err %d\n",r,(int)XML_GetErrorCode(p));
  XML_ParserFree(p); free(b); return 0;}
